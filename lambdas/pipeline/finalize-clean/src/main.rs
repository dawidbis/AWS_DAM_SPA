//! Lambda `finalize-clean` (rola `dam-finalize-clean`): ostatni krok czystej
//! ścieżki `scan-pipeline`. Przenosi plik do bucketu `clean` i ustawia
//! status `CLEAN_DRAFT` (czeka na publikację przez A).
//!
//! Do czasu kroku CDR (etap 2) źródłem jest oryginał z kwarantanny; potem
//! będzie nim zrekonstruowana wersja z `clean/staging`.

use std::sync::Arc;

use lambda_runtime::{Error, LambdaEvent, service_fn};
use serde::Serialize;
use shared::AssetStatus;
use shared::assets::{now_millis, transition_idempotent};
use shared::http::env;
use shared::pipeline::{ScanOutcome, StepInput, move_object, scan_attributes};

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

/// Silnik skanu z wejścia. Krok uruchomiony bez czystego wyniku to błąd
/// konfiguracji maszyny stanów: odmawiamy (fail closed).
fn clean_engine(input: &StepInput) -> Result<&str, String> {
    match &input.scan {
        Some(ScanOutcome::Clean { engine }) => Ok(engine),
        other => Err(format!("finalize-clean bez czystego wyniku skanu: {other:?}")),
    }
}

async fn handler(app: &App, event: LambdaEvent<StepInput>) -> Result<Output, Error> {
    let input = event.payload;
    let asset_id = input.checked_asset_id()?;
    let engine = clean_engine(&input)?;

    move_object(&app.s3, &app.quarantine, &app.clean, asset_id).await?;
    let attributes = scan_attributes("CLEAN", engine, now_millis());
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

    fn input(scan: Option<ScanOutcome>) -> StepInput {
        StepInput {
            asset_id: "0b6f3c1e-8a2d-4f5b-9c7e-1d2a3b4c5d6e".to_owned(),
            scan,
        }
    }

    #[test]
    fn requires_a_clean_verdict() {
        let clean = input(Some(ScanOutcome::Clean {
            engine: "ClamAV".to_owned(),
        }));
        assert_eq!(clean_engine(&clean), Ok("ClamAV"));
        assert!(clean_engine(&input(None)).is_err());
        let infected = input(Some(ScanOutcome::Infected {
            signature: "Eicar".to_owned(),
            engine: "ClamAV".to_owned(),
        }));
        assert!(clean_engine(&infected).is_err());
    }
}
