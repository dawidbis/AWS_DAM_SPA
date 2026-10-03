//! Lambda `handle-infected` (rola `dam-handle-infected`): ścieżka zainfekowana
//! `scan-pipeline`.
//!
//! 1. Plik z kwarantanny trafia do bucketu `infected` (Object Lock: dowód).
//! 2. Status assetu `INFECTED`.
//! 3. Wpis w tabeli `incidents` (jeden incydent na asset).
//! 4. Zdarzenie `asset.infected` w EventBridge; reguła wysyła z niego alert
//!    SNS (rozdział 9). Zdarzenie wychodzi co najmniej raz: znacznik
//!    `alertSentAt` zapisujemy dopiero po udanym wysłaniu.
//!
//! Każdy krok jest idempotentny, więc Step Functions może go ponowić.

use std::collections::HashMap;
use std::sync::Arc;

use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_eventbridge::types::PutEventsRequestEntry;
use lambda_runtime::{Error, LambdaEvent, service_fn};
use serde::Serialize;
use shared::AssetStatus;
use shared::assets::{asset_pk, now_millis, transition_idempotent};
use shared::http::env;
use shared::pipeline::{ScanOutcome, StepInput, move_object, scan_attributes};

/// `source` i `detail-type` zdarzeń domenowych projektu.
const EVENT_SOURCE: &str = "matchday.dam";
const EVENT_INFECTED: &str = "asset.infected";

struct App {
    s3: aws_sdk_s3::Client,
    dynamo: aws_sdk_dynamodb::Client,
    events: aws_sdk_eventbridge::Client,
    assets_table: String,
    incidents_table: String,
    quarantine: String,
    infected: String,
    event_bus: String,
}

/// Szczegóły zdarzenia `asset.infected`. Bez nazwy pliku i innych metadanych
/// od użytkownika (rozdział 8: nie przenosimy ich do logów i powiadomień).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct InfectedDetail {
    incident_id: String,
    asset_id: String,
    uploader_id: String,
    source_ip: Option<String>,
    signature: String,
    engine: String,
    detected_at: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Output {
    asset_id: String,
    status: AssetStatus,
    incident_id: String,
}

fn infected_verdict(input: &StepInput) -> Result<(&str, &str), String> {
    match &input.scan {
        Some(ScanOutcome::Infected { signature, engine }) => Ok((signature, engine)),
        other => Err(format!("handle-infected bez wykrycia: {other:?}")),
    }
}

fn dynamo_error(error: impl std::fmt::Debug) -> String {
    format!("DynamoDB: {error:?}")
}

fn string_attr(item: &HashMap<String, AttributeValue>, name: &str) -> Option<String> {
    item.get(name).and_then(|v| v.as_s().ok()).cloned()
}

/// Autor i adres IP uploadu zapisane przez `upload-init`.
async fn uploader(app: &App, asset_id: &str) -> Result<(String, Option<String>), String> {
    let item = app
        .dynamo
        .get_item()
        .table_name(&app.assets_table)
        .key("pk", AttributeValue::S(asset_pk(asset_id)))
        .projection_expression("uploaderId, uploaderIp")
        .consistent_read(true)
        .send()
        .await
        .map_err(dynamo_error)?
        .item
        .ok_or("asset nie istnieje")?;
    Ok((
        string_attr(&item, "uploaderId").unwrap_or_else(|| "unknown".to_owned()),
        string_attr(&item, "uploaderIp"),
    ))
}

/// Zapisuje incydent, jeśli go jeszcze nie ma. Zwraca, czy alert został
/// już wysłany (poprzednia próba kroku).
async fn record_incident(app: &App, detail: &InfectedDetail) -> Result<bool, String> {
    let mut put = app
        .dynamo
        .put_item()
        .table_name(&app.incidents_table)
        .condition_expression("attribute_not_exists(incidentId)")
        .item("incidentId", AttributeValue::S(detail.incident_id.clone()))
        .item("assetId", AttributeValue::S(detail.asset_id.clone()))
        .item("uploaderId", AttributeValue::S(detail.uploader_id.clone()))
        .item("signature", AttributeValue::S(detail.signature.clone()))
        .item("engine", AttributeValue::S(detail.engine.clone()))
        .item("detectedAt", AttributeValue::N(detail.detected_at.to_string()))
        .item("status", AttributeValue::S("OPEN".to_owned()));
    if let Some(ip) = &detail.source_ip {
        put = put.item("sourceIp", AttributeValue::S(ip.clone()));
    }
    match put.send().await {
        Ok(_) => Ok(false),
        Err(error)
            if error.as_service_error().is_some_and(
                aws_sdk_dynamodb::operation::put_item::PutItemError::is_conditional_check_failed_exception,
            ) =>
        {
            let existing = app
                .dynamo
                .get_item()
                .table_name(&app.incidents_table)
                .key("incidentId", AttributeValue::S(detail.incident_id.clone()))
                .consistent_read(true)
                .send()
                .await
                .map_err(dynamo_error)?;
            Ok(existing.item.is_some_and(|item| item.contains_key("alertSentAt")))
        }
        Err(error) => Err(dynamo_error(error)),
    }
}

async fn publish_alert(app: &App, detail: &InfectedDetail) -> Result<(), String> {
    let entry = PutEventsRequestEntry::builder()
        .event_bus_name(&app.event_bus)
        .source(EVENT_SOURCE)
        .detail_type(EVENT_INFECTED)
        .detail(serde_json::to_string(detail).map_err(|e| e.to_string())?)
        .build();
    let output = app
        .events
        .put_events()
        .entries(entry)
        .send()
        .await
        .map_err(|e| format!("EventBridge: {e:?}"))?;
    if output.failed_entry_count() > 0 {
        return Err(format!("EventBridge odrzucił zdarzenie: {:?}", output.entries()));
    }
    app.dynamo
        .update_item()
        .table_name(&app.incidents_table)
        .key("incidentId", AttributeValue::S(detail.incident_id.clone()))
        .update_expression("SET alertSentAt = :now")
        .expression_attribute_values(":now", AttributeValue::N(now_millis().to_string()))
        .send()
        .await
        .map_err(dynamo_error)?;
    Ok(())
}

async fn handler(app: &App, event: LambdaEvent<StepInput>) -> Result<Output, Error> {
    let input = event.payload;
    let asset_id = input.checked_asset_id()?;
    let (signature, engine) = infected_verdict(&input)?;
    let detected_at = now_millis();

    move_object(&app.s3, &app.quarantine, &app.infected, asset_id).await?;
    let mut attributes = scan_attributes("INFECTED", engine, detected_at);
    attributes.push(("scanSignature", AttributeValue::S(signature.to_owned())));
    transition_idempotent(
        &app.dynamo,
        &app.assets_table,
        asset_id,
        AssetStatus::Infected,
        &attributes,
    )
    .await?;

    let (uploader_id, source_ip) = uploader(app, asset_id).await?;
    let detail = InfectedDetail {
        incident_id: asset_id.to_owned(),
        asset_id: asset_id.to_owned(),
        uploader_id,
        source_ip,
        signature: signature.to_owned(),
        engine: engine.to_owned(),
        detected_at,
    };
    if record_incident(app, &detail).await? {
        tracing::info!(asset_id, "alert already sent");
    } else {
        publish_alert(app, &detail).await?;
    }
    tracing::warn!(asset_id, signature, "infected asset handled");
    Ok(Output {
        asset_id: asset_id.to_owned(),
        status: AssetStatus::Infected,
        incident_id: detail.incident_id,
    })
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    shared::telemetry::init();
    let config = aws_config::load_from_env().await;
    let app = Arc::new(App {
        s3: aws_sdk_s3::Client::new(&config),
        dynamo: aws_sdk_dynamodb::Client::new(&config),
        events: aws_sdk_eventbridge::Client::new(&config),
        assets_table: env("ASSETS_TABLE"),
        incidents_table: env("INCIDENTS_TABLE"),
        quarantine: env("QUARANTINE_BUCKET"),
        infected: env("INFECTED_BUCKET"),
        event_bus: std::env::var("EVENT_BUS_NAME").unwrap_or_else(|_| "default".to_owned()),
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
    fn requires_an_infected_verdict() {
        let input = |scan| StepInput {
            asset_id: "0b6f3c1e-8a2d-4f5b-9c7e-1d2a3b4c5d6e".to_owned(),
            scan,
        };
        let infected = input(Some(ScanOutcome::Infected {
            signature: "Eicar-Test-Signature".to_owned(),
            engine: "ClamAV".to_owned(),
        }));
        assert_eq!(
            infected_verdict(&infected),
            Ok(("Eicar-Test-Signature", "ClamAV"))
        );
        assert!(
            infected_verdict(&input(Some(ScanOutcome::Clean {
                engine: String::new()
            })))
            .is_err()
        );
        assert!(infected_verdict(&input(None)).is_err());
    }

    #[test]
    fn event_detail_carries_only_references() {
        let detail = InfectedDetail {
            incident_id: "a1".to_owned(),
            asset_id: "a1".to_owned(),
            uploader_id: "u1".to_owned(),
            source_ip: Some("203.0.113.7".to_owned()),
            signature: "Eicar".to_owned(),
            engine: "ClamAV".to_owned(),
            detected_at: 1,
        };
        let json = serde_json::to_value(&detail).unwrap();
        let keys: Vec<_> = json.as_object().unwrap().keys().cloned().collect();
        assert_eq!(
            keys,
            [
                "assetId",
                "detectedAt",
                "engine",
                "incidentId",
                "signature",
                "sourceIp",
                "uploaderId"
            ]
        );
    }
}
