//! Lambda `scan`: skan antywirusowy pliku z kwarantanny (etap 1).
//!
//! SQS (zdarzenia S3 z EventBridge) → QUARANTINED→SCANNING → pobranie do /tmp
//! → ClamAV → wynik:
//! - czysty: kopia do `clean`, status CLEAN_DRAFT (czeka na publikację),
//! - zainfekowany: kopia do `infected`, status INFECTED, alert SNS,
//! - błąd lub niepełny skan: status SCAN_FAILED (fail closed, rozdział 7.2).
//!
//! W etapie 2 kroki rozdzieli Step Functions (validate, CDR, osobne role
//! finalize-clean i handle-infected).

mod clamd;
mod event;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use aws_lambda_events::sqs::{BatchItemFailure, SqsBatchResponse, SqsEvent};
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_s3::types::MetadataDirective;
use lambda_runtime::{Error, LambdaEvent, service_fn};
use shared::AssetStatus;
use shared::assets::{StoreError, get_status, now_millis, transition};
use shared::http::env;
use tokio::sync::Mutex;

use crate::clamd::{Clamd, Verdict};
use crate::event::ObjectRef;

struct App {
    s3: aws_sdk_s3::Client,
    dynamo: aws_sdk_dynamodb::Client,
    sns: aws_sdk_sns::Client,
    table: String,
    quarantine: String,
    clean: String,
    infected: String,
    alerts_topic: String,
    clamd: Mutex<Clamd>,
}

/// Błąd, przy którym komunikat wraca do kolejki (np. chwilowy problem z AWS).
#[derive(Debug)]
struct Retry(String);

async fn process(app: &App, object: &ObjectRef) -> Result<(), Retry> {
    if object.bucket != app.quarantine {
        tracing::warn!(bucket = %object.bucket, "event from unexpected bucket, ignoring");
        return Ok(());
    }
    let asset_id = object.asset_id.as_str();

    // Idempotencja: powtórzone zdarzenie zastanie inny status i zostanie
    // pominięte. SCANNING = poprzednia próba przerwana (timeout), skanujemy ponownie.
    match transition(&app.dynamo, &app.table, asset_id, AssetStatus::Scanning, &[]).await {
        Ok(()) => {}
        Err(StoreError::InvalidTransition(_)) => match get_status(&app.dynamo, &app.table, asset_id).await {
            Ok((AssetStatus::Scanning, _)) => tracing::info!(asset_id, "resuming interrupted scan"),
            Ok((status, _)) => {
                tracing::info!(asset_id, %status, "asset not awaiting scan, skipping duplicate event");
                return Ok(());
            }
            Err(StoreError::NotFound) => {
                tracing::warn!(asset_id, "object without asset record, skipping");
                return Ok(());
            }
            Err(error) => return Err(Retry(error.to_string())),
        },
        Err(error) => return Err(Retry(error.to_string())),
    }

    let verdict = scan(app, asset_id).await;
    let engine = app.clamd.lock().await.version().await;
    tracing::info!(asset_id, ?verdict, %engine, "scan finished");

    let result = match &verdict {
        Verdict::Clean => finish_clean(app, asset_id, &engine).await,
        Verdict::Infected(signature) => finish_infected(app, asset_id, signature, &engine).await,
        Verdict::Failed(reason) => mark_failed(app, asset_id, reason).await,
    };
    if let Err(error) = result {
        // Nie udało się zapisać wyniku: plik zostaje niedostępny (fail closed).
        tracing::error!(asset_id, %error, "finishing scan failed");
        mark_failed(app, asset_id, &format!("finalizacja: {error}"))
            .await
            .map_err(Retry)?;
    }
    Ok(())
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

async fn move_object(app: &App, asset_id: &str, target_bucket: &str) -> Result<(), String> {
    app.s3
        .copy_object()
        .copy_source(format!("{}/{asset_id}", app.quarantine))
        .bucket(target_bucket)
        .key(asset_id)
        .metadata_directive(MetadataDirective::Copy)
        .send()
        .await
        .map_err(|e| format!("kopiowanie do {target_bucket}: {e:?}"))?;
    app.s3
        .delete_object()
        .bucket(&app.quarantine)
        .key(asset_id)
        .send()
        .await
        .map_err(|e| format!("usunięcie z kwarantanny: {e:?}"))?;
    Ok(())
}

fn scan_attributes(verdict: &str, engine: &str) -> Vec<(&'static str, AttributeValue)> {
    vec![
        ("scanVerdict", AttributeValue::S(verdict.to_owned())),
        ("scanEngine", AttributeValue::S(engine.to_owned())),
        ("scannedAt", AttributeValue::N(now_millis().to_string())),
    ]
}

async fn finish_clean(app: &App, asset_id: &str, engine: &str) -> Result<(), String> {
    move_object(app, asset_id, &app.clean).await?;
    let attributes = scan_attributes("CLEAN", engine);
    transition(
        &app.dynamo,
        &app.table,
        asset_id,
        AssetStatus::CleanDraft,
        &attributes,
    )
    .await
    .map_err(|e| e.to_string())
}

async fn finish_infected(app: &App, asset_id: &str, signature: &str, engine: &str) -> Result<(), String> {
    move_object(app, asset_id, &app.infected).await?;
    let mut attributes = scan_attributes("INFECTED", engine);
    attributes.push(("scanSignature", AttributeValue::S(signature.to_owned())));
    transition(
        &app.dynamo,
        &app.table,
        asset_id,
        AssetStatus::Infected,
        &attributes,
    )
    .await
    .map_err(|e| e.to_string())?;

    let uploader = get_status(&app.dynamo, &app.table, asset_id)
        .await
        .map(|(_, uploader)| uploader)
        .ok();
    let message = format!(
        "Wykryto złośliwy plik w Matchday DAM.\n\nAsset: {asset_id}\nUploader (sub): {}\nSygnatura: {signature}\nSilnik: {engine}\n\nPlik przeniesiono do bucketu infected; nie jest dostępny w galerii.",
        uploader.as_deref().unwrap_or("nieznany")
    );
    app.sns
        .publish()
        .topic_arn(&app.alerts_topic)
        .subject("Matchday DAM: wykryto złośliwy plik")
        .message(message)
        .send()
        .await
        .map_err(|e| format!("alert SNS: {e:?}"))?;
    Ok(())
}

async fn mark_failed(app: &App, asset_id: &str, reason: &str) -> Result<(), String> {
    let mut attributes = scan_attributes("FAILED", "n/a");
    attributes.push(("scanError", AttributeValue::S(reason.chars().take(500).collect())));
    match transition(
        &app.dynamo,
        &app.table,
        asset_id,
        AssetStatus::ScanFailed,
        &attributes,
    )
    .await
    {
        Ok(()) | Err(StoreError::InvalidTransition(_)) => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

async fn handler(app: &App, event: LambdaEvent<SqsEvent>) -> Result<SqsBatchResponse, Error> {
    let mut response = SqsBatchResponse::default();
    for record in event.payload.records {
        let id = record.message_id.clone().unwrap_or_default();
        let object = match event::parse(record.body.as_deref().unwrap_or_default()) {
            Ok(object) => object,
            Err(error) => {
                // Komunikat, którego nie rozumiemy, nie wróci do kolejki w nieskończoność.
                tracing::warn!(message_id = %id, %error, "ignoring message");
                continue;
            }
        };
        if let Err(Retry(error)) = process(app, &object).await {
            tracing::error!(message_id = %id, asset_id = %object.asset_id, %error, "will retry");
            let mut failure = BatchItemFailure::default();
            failure.item_identifier = id;
            response.batch_item_failures.push(failure);
        }
    }
    Ok(response)
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    shared::telemetry::init();
    let config = aws_config::load_from_env().await;
    let app = Arc::new(App {
        s3: aws_sdk_s3::Client::new(&config),
        dynamo: aws_sdk_dynamodb::Client::new(&config),
        sns: aws_sdk_sns::Client::new(&config),
        table: env("ASSETS_TABLE"),
        quarantine: env("QUARANTINE_BUCKET"),
        clean: env("CLEAN_BUCKET"),
        infected: env("INFECTED_BUCKET"),
        alerts_topic: env("ALERTS_TOPIC_ARN"),
        clamd: Mutex::new(Clamd::new(
            "/tmp/clamd.sock",
            std::env::var("CLAMD_CONFIG").unwrap_or_else(|_| "/etc/clamav/clamd.conf".to_owned()),
            Duration::from_mins(3),
        )),
    });
    lambda_runtime::run(service_fn(move |event: LambdaEvent<SqsEvent>| {
        let app = Arc::clone(&app);
        async move { handler(&app, event).await }
    }))
    .await
}
