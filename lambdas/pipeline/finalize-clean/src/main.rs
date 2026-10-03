//! Lambda `finalize-clean` (rola `dam-finalize-clean`): ostatni krok czystej
//! ścieżki `scan-pipeline`.
//!
//! Zrekonstruowany plik z `clean/staging/<id>` (krok CDR) staje się
//! publikowalną kopią `clean/<id>`, oryginał znika z kwarantanny, a asset
//! dostaje status `CLEAN_DRAFT` z danymi ustalonymi przez pipeline: typem
//! z magic bytes, wymiarami, rozmiarem i SHA-256 zrekonstruowanego pliku
//! oraz przepuszczonymi polami EXIF (rozdział 5: nie od klienta).
//! Znacznik `hasRenditions` mówi galerii, że są miniatura i podgląd.

use std::sync::Arc;

use aws_sdk_dynamodb::types::AttributeValue;
use lambda_runtime::{Error, LambdaEvent, service_fn};
use serde::Serialize;
use shared::AssetStatus;
use shared::assets::{now_millis, transition_idempotent};
use shared::http::env;
use shared::pipeline::{
    DisarmOutcome, Location, ScanOutcome, StepInput, ValidationOutcome, delete_object, move_object,
    scan_attributes, staging_key,
};

struct App {
    s3: aws_sdk_s3::Client,
    dynamo: aws_sdk_dynamodb::Client,
    table: String,
    quarantine: String,
    clean: String,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Output {
    asset_id: String,
    status: AssetStatus,
}

/// Atrybuty zapisywane przy assecie. Krok uruchomiony bez kompletu
/// pozytywnych wyników (skan, walidacja, CDR) to błąd konfiguracji maszyny
/// stanów: odmawiamy (fail closed).
fn clean_attributes(input: &StepInput, now: u64) -> Result<Vec<(&'static str, AttributeValue)>, String> {
    let Some(ScanOutcome::Clean { engine }) = &input.scan else {
        return Err(format!("finalize-clean bez czystego skanu: {:?}", input.scan));
    };
    let Some(ValidationOutcome::Valid {
        detected_type,
        width,
        height,
        size_bytes: original_size,
    }) = &input.validation
    else {
        return Err(format!("finalize-clean bez walidacji: {:?}", input.validation));
    };
    let Some(DisarmOutcome::Clean {
        size_bytes,
        sha256,
        metadata,
    }) = &input.disarm
    else {
        return Err(format!("finalize-clean bez CDR: {:?}", input.disarm));
    };
    if input.renditions.is_none() {
        return Err("finalize-clean bez miniatury i podglądu".to_owned());
    }

    let n = |value: u64| AttributeValue::N(value.to_string());
    let mut attributes = scan_attributes("CLEAN", engine, now);
    attributes.extend([
        ("detectedType", AttributeValue::S(detected_type.clone())),
        ("width", n(u64::from(*width))),
        ("height", n(u64::from(*height))),
        ("originalSizeBytes", n(*original_size)),
        ("sizeBytes", n(*size_bytes)),
        ("sha256", AttributeValue::S(sha256.clone())),
        ("disarmedAt", n(now)),
        ("hasRenditions", AttributeValue::Bool(true)),
    ]);
    for (name, value) in [
        ("exifArtist", &metadata.artist),
        ("exifCopyright", &metadata.copyright),
        ("exifTakenAt", &metadata.taken_at),
    ] {
        if let Some(value) = value {
            attributes.push((name, AttributeValue::S(value.clone())));
        }
    }
    Ok(attributes)
}

async fn handler(app: &App, event: LambdaEvent<StepInput>) -> Result<Output, Error> {
    let input = event.payload;
    let asset_id = input.checked_asset_id()?;
    let attributes = clean_attributes(&input, now_millis())?;

    let staging = staging_key(asset_id);
    move_object(
        &app.s3,
        Location {
            bucket: &app.clean,
            key: &staging,
        },
        Location {
            bucket: &app.clean,
            key: asset_id,
        },
    )
    .await?;
    // Oryginał od użytkownika nie jest już potrzebny (do galerii trafia wersja po CDR).
    delete_object(
        &app.s3,
        Location {
            bucket: &app.quarantine,
            key: asset_id,
        },
    )
    .await?;
    transition_idempotent(
        &app.dynamo,
        &app.table,
        asset_id,
        AssetStatus::CleanDraft,
        &attributes,
    )
    .await?;
    tracing::info!(asset_id, "asset clean");
    Ok(Output {
        asset_id: asset_id.to_owned(),
        status: AssetStatus::CleanDraft,
    })
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
        clean: env("CLEAN_BUCKET"),
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
    use shared::pipeline::{PreservedMetadata, RenditionsOutcome};

    fn complete() -> StepInput {
        StepInput {
            scan: Some(ScanOutcome::Clean {
                engine: "ClamAV".to_owned(),
            }),
            validation: Some(ValidationOutcome::Valid {
                detected_type: "image/jpeg".to_owned(),
                width: 4000,
                height: 3000,
                size_bytes: 5_000_000,
            }),
            disarm: Some(DisarmOutcome::Clean {
                size_bytes: 4_000_000,
                sha256: "ab".repeat(32),
                metadata: PreservedMetadata {
                    artist: Some("Jan Fotograf".to_owned()),
                    ..PreservedMetadata::default()
                },
            }),
            renditions: Some(RenditionsOutcome {
                thumbnail_key: "thumb/x.jpg".to_owned(),
                preview_key: "preview/x.jpg".to_owned(),
            }),
            ..StepInput::new("0b6f3c1e-8a2d-4f5b-9c7e-1d2a3b4c5d6e")
        }
    }

    #[test]
    fn records_facts_established_by_the_pipeline() {
        let attributes = clean_attributes(&complete(), 1).unwrap();
        let get = |name: &str| {
            attributes
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| v.clone())
        };
        assert_eq!(
            get("detectedType"),
            Some(AttributeValue::S("image/jpeg".to_owned()))
        );
        assert_eq!(get("sizeBytes"), Some(AttributeValue::N("4000000".to_owned())));
        assert_eq!(
            get("exifArtist"),
            Some(AttributeValue::S("Jan Fotograf".to_owned()))
        );
        assert_eq!(get("exifCopyright"), None);
    }

    #[test]
    fn refuses_without_every_positive_result() {
        let mut no_disarm = complete();
        no_disarm.disarm = Some(DisarmOutcome::Rejected {
            reason: "x".to_owned(),
        });
        assert!(clean_attributes(&no_disarm, 1).is_err());

        let mut infected = complete();
        infected.scan = Some(ScanOutcome::Infected {
            signature: "Eicar".to_owned(),
            engine: "ClamAV".to_owned(),
        });
        assert!(clean_attributes(&infected, 1).is_err());

        let mut no_renditions = complete();
        no_renditions.renditions = None;
        assert!(clean_attributes(&no_renditions, 1).is_err());

        let mut not_validated = complete();
        not_validated.validation = None;
        assert!(clean_attributes(&not_validated, 1).is_err());
    }
}
