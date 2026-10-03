//! Lambda `scan`: krok skanu antywirusowego w Step Functions `scan-pipeline`.
//!
//! Wejście `{ "assetId" }`, wyjście `ScanOutcome` (CLEAN / INFECTED / FAILED).
//! Funkcja tylko czyta plik z kwarantanny; przeniesienie pliku i zmiany
//! statusu wykonują kolejne kroki z własnymi rolami (rozdział 10.1).
//! Każdy błąd to FAILED, czyli fail closed (rozdział 7.2).

mod clamd;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use lambda_runtime::{Error, LambdaEvent, service_fn};
use shared::http::env;
use shared::pipeline::{ScanOutcome, StepInput};
use tokio::sync::Mutex;

use crate::clamd::{Clamd, Verdict};

struct App {
    s3: aws_sdk_s3::Client,
    quarantine: String,
    clamd: Mutex<Clamd>,
}

/// Pobiera obiekt do /tmp i skanuje go. Każdy błąd to `Verdict::Failed`.
async fn scan(app: &App, asset_id: &str) -> Verdict {
    let path = PathBuf::from(format!("/tmp/scan-{asset_id}"));
    let verdict = async {
        let object = app
            .s3
            .get_object()
            .bucket(&app.quarantine)
            .key(asset_id)
            .send()
            .await
            .map_err(|e| format!("pobranie z kwarantanny: {e:?}"))?;
        let mut reader = object.body.into_async_read();
        let mut file = tokio::fs::File::create(&path).await.map_err(|e| e.to_string())?;
        tokio::io::copy(&mut reader, &mut file)
            .await
            .map_err(|e| format!("zapis do /tmp: {e}"))?;
        drop(file);

        let mut clamd = app.clamd.lock().await;
        clamd.ensure_started().await?;
        Ok::<_, String>(clamd.scan(&path).await)
    }
    .await
    .unwrap_or_else(Verdict::Failed);
    let _ = tokio::fs::remove_file(&path).await;
    verdict
}

fn outcome(verdict: Verdict, engine: String) -> ScanOutcome {
    match verdict {
        Verdict::Clean => ScanOutcome::Clean { engine },
        Verdict::Infected(signature) => ScanOutcome::Infected { signature, engine },
        Verdict::Failed(reason) => ScanOutcome::Failed {
            reason: reason.chars().take(500).collect(),
        },
    }
}

async fn handler(app: &App, event: LambdaEvent<StepInput>) -> Result<ScanOutcome, Error> {
    let asset_id = match event.payload.checked_asset_id() {
        Ok(asset_id) => asset_id.to_owned(),
        Err(reason) => return Ok(ScanOutcome::Failed { reason }),
    };
    let verdict = scan(app, &asset_id).await;
    let engine = app.clamd.lock().await.version().await;
    tracing::info!(asset_id, ?verdict, %engine, "scan finished");
    Ok(outcome(verdict, engine))
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    shared::telemetry::init();
    let config = aws_config::load_from_env().await;
    let app = Arc::new(App {
        s3: aws_sdk_s3::Client::new(&config),
        quarantine: env("QUARANTINE_BUCKET"),
        clamd: Mutex::new(Clamd::new(
            "/tmp/clamd.sock",
            std::env::var("CLAMD_CONFIG").unwrap_or_else(|_| "/etc/clamav/clamd.conf".to_owned()),
            Duration::from_mins(3),
        )),
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

    #[test]
    fn maps_verdicts_to_pipeline_outcomes() {
        assert_eq!(
            outcome(Verdict::Clean, "ClamAV".to_owned()),
            ScanOutcome::Clean {
                engine: "ClamAV".to_owned()
            }
        );
        assert!(matches!(
            outcome(Verdict::Infected("Eicar".to_owned()), "ClamAV".to_owned()),
            ScanOutcome::Infected { signature, .. } if signature == "Eicar"
        ));
        let ScanOutcome::Failed { reason } = outcome(Verdict::Failed("x".repeat(2000)), String::new()) else {
            panic!("expected failure");
        };
        assert_eq!(reason.len(), 500);
    }
}
