//! Lambda `renditions` (rola `dam-renditions`): krok `scan-pipeline` po CDR.
//!
//! Z wersji po CDR (`clean/staging/<id>`) powstają dwie wersje w buckecie
//! `renditions` (rozdział 9):
//! - `thumb/<id>.jpg`: miniatura do kafelków galerii (A, B), zamiast
//!   ładowania oryginału,
//! - `preview/<id>.jpg`: podgląd w niskiej rozdzielczości ze znakiem wodnym,
//!   jedyna wersja, jaką widzi grupa D (rozdział 4: nigdy oryginał).

mod watermark;

use std::io::Cursor;
use std::sync::Arc;

use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageFormat, ImageReader, Limits};
use lambda_runtime::{Error, LambdaEvent, service_fn};
use shared::http::env;
use shared::pipeline::limits::MAX_DIMENSION;
use shared::pipeline::{
    DisarmOutcome, RenditionsOutcome, StepInput, ValidationOutcome, preview_key, staging_key, thumbnail_key,
};

/// Najdłuższy bok miniatury i podglądu.
const THUMBNAIL_SIZE: u32 = 400;
const PREVIEW_SIZE: u32 = 1200;
const THUMBNAIL_QUALITY: u8 = 80;
const PREVIEW_QUALITY: u8 = 75;
const MAX_DECODER_ALLOC: u64 = 1024 * 1024 * 1024;

struct App {
    s3: aws_sdk_s3::Client,
    clean: String,
    renditions: String,
}

#[derive(Debug)]
struct Rendered {
    thumbnail: Vec<u8>,
    preview: Vec<u8>,
}

fn jpeg(image: &DynamicImage, quality: u8) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    DynamicImage::ImageRgb8(image.to_rgb8())
        .write_with_encoder(JpegEncoder::new_with_quality(&mut out, quality))
        .map_err(|e| format!("kodowanie JPEG: {e}"))?;
    Ok(out)
}

/// Zmniejsza obraz tak, żeby dłuższy bok miał najwyżej `max` px (bez
/// powiększania małych obrazów).
fn fit(image: &DynamicImage, max: u32) -> DynamicImage {
    if image.width() <= max && image.height() <= max {
        image.clone()
    } else {
        image.thumbnail(max, max)
    }
}

/// Miniatura i podgląd ze znakiem wodnym z obrazu po CDR.
fn render(bytes: &[u8], format: ImageFormat) -> Result<Rendered, String> {
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(MAX_DECODER_ALLOC);
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits);
    let image = reader.decode().map_err(|e| format!("dekodowanie: {e}"))?;

    let thumbnail = fit(&image, THUMBNAIL_SIZE);
    let mut preview = fit(&image, PREVIEW_SIZE).to_rgb8();
    watermark::apply(&mut preview);
    Ok(Rendered {
        thumbnail: jpeg(&thumbnail, THUMBNAIL_QUALITY)?,
        preview: jpeg(&DynamicImage::ImageRgb8(preview), PREVIEW_QUALITY)?,
    })
}

fn format_of(detected_type: &str) -> Result<ImageFormat, String> {
    ImageFormat::from_mime_type(detected_type).ok_or_else(|| format!("brak renderowania dla {detected_type}"))
}

async fn put(app: &App, key: &str, bytes: Vec<u8>) -> Result<(), String> {
    app.s3
        .put_object()
        .bucket(&app.renditions)
        .key(key)
        .content_type("image/jpeg")
        .cache_control("private, max-age=300")
        .body(bytes.into())
        .send()
        .await
        .map(|_| ())
        .map_err(|e| format!("zapis {key}: {e:?}"))
}

async fn handler(app: &App, event: LambdaEvent<StepInput>) -> Result<RenditionsOutcome, Error> {
    let input = event.payload;
    let asset_id = input.checked_asset_id()?;
    let Some(ValidationOutcome::Valid { detected_type, .. }) = &input.validation else {
        return Err("renditions bez walidacji".into());
    };
    if !matches!(input.disarm, Some(DisarmOutcome::Clean { .. })) {
        return Err("renditions bez wersji po CDR".into());
    }

    let reconstructed = app
        .s3
        .get_object()
        .bucket(&app.clean)
        .key(staging_key(asset_id))
        .send()
        .await
        .map_err(|e| format!("odczyt clean/staging: {e:?}"))?
        .body
        .collect()
        .await
        .map_err(|e| format!("odczyt clean/staging: {e}"))?
        .into_bytes();

    let rendered = render(&reconstructed, format_of(detected_type)?)?;
    let outcome = RenditionsOutcome {
        thumbnail_key: thumbnail_key(asset_id),
        preview_key: preview_key(asset_id),
    };
    put(app, &outcome.thumbnail_key, rendered.thumbnail).await?;
    put(app, &outcome.preview_key, rendered.preview).await?;
    tracing::info!(asset_id, "renditions created");
    Ok(outcome)
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    shared::telemetry::init();
    let config = aws_config::load_from_env().await;
    let app = Arc::new(App {
        s3: aws_sdk_s3::Client::new(&config),
        clean: env("CLEAN_BUCKET"),
        renditions: env("RENDITIONS_BUCKET"),
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
    use image::codecs::png::PngEncoder;
    use image::{GenericImageView, Rgb, RgbImage};

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut out = Vec::new();
        DynamicImage::ImageRgb8(RgbImage::from_pixel(width, height, Rgb([30, 90, 160])))
            .write_with_encoder(PngEncoder::new(&mut out))
            .unwrap();
        out
    }

    #[test]
    fn renders_small_jpegs_within_size_limits() {
        let rendered = render(&png(3000, 2000), ImageFormat::Png).unwrap();
        let thumbnail = image::load_from_memory_with_format(&rendered.thumbnail, ImageFormat::Jpeg).unwrap();
        let preview = image::load_from_memory_with_format(&rendered.preview, ImageFormat::Jpeg).unwrap();
        assert_eq!(thumbnail.dimensions(), (400, 267));
        assert_eq!(preview.dimensions(), (1200, 800));
    }

    #[test]
    fn preview_carries_the_watermark_and_thumbnail_does_not() {
        let rendered = render(&png(1200, 800), ImageFormat::Png).unwrap();
        let preview = image::load_from_memory(&rendered.preview).unwrap().to_rgb8();
        let thumbnail = image::load_from_memory(&rendered.thumbnail).unwrap().to_rgb8();
        let distinct = |img: &RgbImage| {
            img.pixels()
                .filter(|p| p.0.iter().zip([30u8, 90, 160]).any(|(a, b)| a.abs_diff(b) > 40))
                .count()
        };
        assert!(distinct(&preview) > 1000, "podgląd bez znaku wodnego");
        assert_eq!(distinct(&thumbnail), 0);
    }

    #[test]
    fn keeps_small_images_small() {
        let rendered = render(&png(200, 100), ImageFormat::Png).unwrap();
        let preview = image::load_from_memory(&rendered.preview).unwrap();
        assert_eq!(preview.dimensions(), (200, 100));
    }

    #[test]
    fn rejects_unknown_formats_and_garbage() {
        assert!(format_of("application/pdf").is_err());
        assert!(render(b"not an image", ImageFormat::Png).is_err());
    }
}
