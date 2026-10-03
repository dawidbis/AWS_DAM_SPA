//! Lambda `validate` (rola `dam-validate`): krok `scan-pipeline` po skanie
//! antywirusowym, przed CDR.
//!
//! Zero zaufania do klienta (rozdział 7.2): typ pliku ustalamy z magic bytes
//! początku pliku, a deklaracja z `upload-init` służy tylko do porównania.
//! Wymiary obrazu czytamy z nagłówka, bez dekodowania pikseli, więc bomba
//! dekompresyjna zostaje odrzucona, zanim cokolwiek ją rozpakuje
//! (scenariusz 6).

use std::sync::Arc;

use aws_sdk_dynamodb::types::AttributeValue;
use lambda_runtime::{Error, LambdaEvent, service_fn};
use shared::assets::asset_pk;
use shared::http::env;
use shared::pipeline::limits::{MAX_DIMENSION, MAX_IMAGE_BYTES, MAX_PIXELS};
use shared::pipeline::{StepInput, ValidationOutcome};
use shared::upload::ALLOWED_CONTENT_TYPES;

/// Ile bajtów początku pliku czytamy. Nagłówek JPEG (SOF) bywa za segmentami
/// APP z miniaturą EXIF i XMP, więc 64 KB nie zawsze wystarcza.
const HEADER_BYTES: u64 = 1024 * 1024;

struct App {
    s3: aws_sdk_s3::Client,
    dynamo: aws_sdk_dynamodb::Client,
    table: String,
    quarantine: String,
}

fn rejected(reason: impl Into<String>) -> ValidationOutcome {
    ValidationOutcome::Rejected {
        reason: reason.into(),
    }
}

/// Decyzja na podstawie początku pliku, deklaracji klienta i faktycznego
/// rozmiaru obiektu w S3.
fn validate(header: &[u8], declared_type: &str, size_bytes: u64) -> ValidationOutcome {
    if size_bytes == 0 {
        return rejected("pusty plik");
    }
    if size_bytes > MAX_IMAGE_BYTES {
        return rejected(format!("plik większy niż {} MB", MAX_IMAGE_BYTES / 1024 / 1024));
    }
    let Some(detected) = infer::get(header).map(|kind| kind.mime_type()) else {
        return rejected("nierozpoznany typ pliku");
    };
    if !ALLOWED_CONTENT_TYPES.contains(&detected) {
        return rejected(format!("typ {detected} spoza dozwolonych"));
    }
    if detected != declared_type {
        return rejected(format!("typ {detected} niezgodny z deklaracją {declared_type}"));
    }
    let Ok(size) = imagesize::blob_size(header) else {
        return rejected("nie można odczytać wymiarów obrazu z nagłówka");
    };
    let (Ok(width), Ok(height)) = (u32::try_from(size.width), u32::try_from(size.height)) else {
        return rejected("wymiary obrazu poza zakresem");
    };
    if width == 0 || height == 0 {
        return rejected("obraz bez wymiarów");
    }
    if width > MAX_DIMENSION || height > MAX_DIMENSION || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return rejected(format!("obraz {width}×{height} przekracza limit wymiarów"));
    }
    ValidationOutcome::Valid {
        detected_type: detected.to_owned(),
        width,
        height,
        size_bytes,
    }
}

async fn declared_type(app: &App, asset_id: &str) -> Result<String, String> {
    app.dynamo
        .get_item()
        .table_name(&app.table)
        .key("pk", AttributeValue::S(asset_pk(asset_id)))
        .projection_expression("declaredContentType")
        .consistent_read(true)
        .send()
        .await
        .map_err(|e| format!("DynamoDB: {e:?}"))?
        .item
        .and_then(|item| {
            item.get("declaredContentType")
                .and_then(|v| v.as_s().ok())
                .cloned()
        })
        .ok_or_else(|| "brak deklarowanego typu".to_owned())
}

async fn header(app: &App, asset_id: &str) -> Result<(Vec<u8>, u64), String> {
    let object = app
        .s3
        .get_object()
        .bucket(&app.quarantine)
        .key(asset_id)
        .range(format!("bytes=0-{}", HEADER_BYTES - 1))
        .send()
        .await
        .map_err(|e| format!("odczyt z kwarantanny: {e:?}"))?;
    // Content-Range: bytes 0-1048575/<rozmiar całego obiektu>.
    let total = object
        .content_range()
        .and_then(|range| range.rsplit('/').next())
        .and_then(|total| total.parse::<u64>().ok())
        .ok_or("brak rozmiaru obiektu w Content-Range")?;
    let bytes = object
        .body
        .collect()
        .await
        .map_err(|e| format!("odczyt z kwarantanny: {e}"))?
        .into_bytes()
        .to_vec();
    Ok((bytes, total))
}

async fn handler(app: &App, event: LambdaEvent<StepInput>) -> Result<ValidationOutcome, Error> {
    let input = event.payload;
    let asset_id = input.checked_asset_id()?;
    let declared = declared_type(app, asset_id).await?;
    let (head, size) = header(app, asset_id).await?;
    let outcome = validate(&head, &declared, size);
    tracing::info!(asset_id, ?outcome, "validation finished");
    Ok(outcome)
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    shared::telemetry::init();
    let config = aws_config::load_from_env().await;
    let app = Arc::new(App {
        s3: aws_sdk_s3::Client::new(&config),
        dynamo: aws_sdk_dynamodb::Client::new(&config),
        table: env("ASSETS_TABLE"),
        quarantine: env("QUARANTINE_BUCKET"),
    });
    lambda_runtime::run(service_fn(move |event: LambdaEvent<StepInput>| {
        let app = Arc::clone(&app);
        async move { handler(&app, event).await }
    }))
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimalny nagłówek PNG z podanymi wymiarami (IHDR).
    fn png_header(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
        bytes
    }

    fn is_rejected(outcome: &ValidationOutcome) -> bool {
        matches!(outcome, ValidationOutcome::Rejected { .. })
    }

    #[test]
    fn accepts_a_png_matching_its_declaration() {
        assert_eq!(
            validate(&png_header(640, 480), "image/png", 1234),
            ValidationOutcome::Valid {
                detected_type: "image/png".to_owned(),
                width: 640,
                height: 480,
                size_bytes: 1234
            }
        );
    }

    #[test]
    fn rejects_an_executable_renamed_to_jpg() {
        // Scenariusz 4: nagłówek PE (MZ) zadeklarowany jako image/jpeg.
        let exe = b"MZ\x90\x00\x03\x00\x00\x00\x04\x00\x00\x00\xff\xff\x00\x00";
        assert!(is_rejected(&validate(exe, "image/jpeg", 1000)));
    }

    #[test]
    fn rejects_svg_with_script() {
        // Scenariusz 3: SVG to tekst, nie ma go na liście dozwolonych typów.
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#;
        assert!(is_rejected(&validate(svg, "image/png", 100)));
    }

    #[test]
    fn rejects_a_type_that_differs_from_the_declaration() {
        assert!(is_rejected(&validate(&png_header(10, 10), "image/jpeg", 100)));
    }

    #[test]
    fn rejects_decompression_bombs_before_decoding() {
        // Scenariusz 6: mały plik deklarujący ogromne wymiary.
        assert!(is_rejected(&validate(
            &png_header(100_000, 100_000),
            "image/png",
            2000
        )));
        assert!(is_rejected(&validate(
            &png_header(15_000, 15_000),
            "image/png",
            2000
        )));
        assert!(is_rejected(&validate(&png_header(0, 10), "image/png", 2000)));
    }

    #[test]
    fn rejects_empty_and_oversized_files() {
        assert!(is_rejected(&validate(&png_header(10, 10), "image/png", 0)));
        assert!(is_rejected(&validate(
            &png_header(10, 10),
            "image/png",
            MAX_IMAGE_BYTES + 1
        )));
    }
}
