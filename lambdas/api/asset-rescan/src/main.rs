//! `POST /assets/{assetId}/rescan`: ponowienie skanu przez A po `SCAN_FAILED`
//! (rola `dam-asset-rescan`).
//!
//! Status zmienia się warunkowo `SCAN_FAILED` → `SCANNING`, a potem startuje
//! nowe wykonanie `scan-pipeline` z flagą `marked` (maszyna stanów pomija
//! wtedy oznaczenie skanu). Plik musi jeszcze być w kwarantannie (lifecycle
//! usuwa go po kilku dniach); jeśli go nie ma, skan znowu skończy się
//! `SCAN_FAILED`.

use std::sync::Arc;

use lambda_http::{Error, Request, http::StatusCode, service_fn};
use serde::Serialize;
use shared::assets::{StoreError, now_millis, transition};
use shared::catalog::{AssetStatusResponse, is_asset_id};
use shared::http::{self, ApiError};
use shared::pipeline::execution_name;
use shared::{AssetStatus, UserGroup};

struct App {
    dynamo: aws_sdk_dynamodb::Client,
    sfn: aws_sdk_sfn::Client,
    table: String,
    state_machine_arn: String,
}

/// Wejście wykonania: asset już ma status SCANNING.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RescanInput<'a> {
    asset_id: &'a str,
    marked: bool,
}

async fn start_pipeline(app: &App, asset_id: &str) -> Result<(), String> {
    let input = serde_json::to_string(&RescanInput {
        asset_id,
        marked: true,
    })
    .map_err(|e| e.to_string())?;
    app.sfn
        .start_execution()
        .state_machine_arn(&app.state_machine_arn)
        .name(execution_name(asset_id, Some(now_millis())))
        .input(input)
        .send()
        .await
        .map(|_| ())
        .map_err(|e| format!("{e:?}"))
}

async fn handle(app: &App, request: &Request) -> Result<(StatusCode, AssetStatusResponse), ApiError> {
    let caller = http::caller(request)?;
    caller.require_any_group(&[UserGroup::Admin])?;
    let asset_id = http::path_param(request, "assetId")?;
    if !is_asset_id(&asset_id) {
        return Err(ApiError::NotFound);
    }

    match transition(&app.dynamo, &app.table, &asset_id, AssetStatus::Scanning, &[]).await {
        Ok(()) => {}
        Err(StoreError::InvalidTransition(_) | StoreError::NotFound) => {
            return Err(ApiError::Conflict(
                "Only assets with a failed scan can be rescanned".to_owned(),
            ));
        }
        Err(error) => return Err(ApiError::internal(error)),
    }
    if let Err(error) = start_pipeline(app, &asset_id).await {
        // Bez wykonania asset utknąłby w SCANNING: wracamy do SCAN_FAILED.
        let _ = transition(&app.dynamo, &app.table, &asset_id, AssetStatus::ScanFailed, &[]).await;
        return Err(ApiError::internal(error));
    }
    tracing::info!(asset_id, caller = %caller.sub, "rescan started");
    Ok((
        StatusCode::ACCEPTED,
        AssetStatusResponse {
            asset_id,
            status: AssetStatus::Scanning,
        },
    ))
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    shared::telemetry::init();
    let config = aws_config::load_from_env().await;
    let app = Arc::new(App {
        dynamo: aws_sdk_dynamodb::Client::new(&config),
        sfn: aws_sdk_sfn::Client::new(&config),
        table: http::env("ASSETS_TABLE"),
        state_machine_arn: http::env("STATE_MACHINE_ARN"),
    });
    lambda_http::run(service_fn(move |request: Request| {
        let app = Arc::clone(&app);
        async move { Ok::<_, Error>(http::respond(handle(&app, &request).await)) }
    }))
    .await
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use lambda_http::RequestExt;
    use shared::http::testing::request_as;

    fn app() -> App {
        let dynamo = aws_sdk_dynamodb::Config::builder()
            .behavior_version(aws_sdk_dynamodb::config::BehaviorVersion::latest())
            .region(aws_sdk_dynamodb::config::Region::new("eu-central-1"))
            .credentials_provider(aws_sdk_dynamodb::config::Credentials::for_tests())
            .build();
        let sfn = aws_sdk_sfn::Config::builder()
            .behavior_version(aws_sdk_sfn::config::BehaviorVersion::latest())
            .region(aws_sdk_sfn::config::Region::new("eu-central-1"))
            .credentials_provider(aws_sdk_sfn::config::Credentials::for_tests())
            .build();
        App {
            dynamo: aws_sdk_dynamodb::Client::from_conf(dynamo),
            sfn: aws_sdk_sfn::Client::from_conf(sfn),
            table: "assets".to_owned(),
            state_machine_arn: "arn:aws:states:eu-central-1:123456789012:stateMachine:scan".to_owned(),
        }
    }

    fn request(groups: &str, asset_id: &str) -> Request {
        request_as("u1", groups)
            .with_path_parameters(HashMap::from([("assetId".to_owned(), asset_id.to_owned())]))
    }

    #[tokio::test]
    async fn only_admins_can_rescan() {
        for groups in ["[staff]", "[contributor]", "[viewer]"] {
            let result = handle(&app(), &request(groups, "0b5e6a1c-2f3d-4e5f-8a9b-0c1d2e3f4a5b")).await;
            assert!(matches!(result, Err(ApiError::Forbidden)), "{groups}");
        }
    }

    #[tokio::test]
    async fn malformed_ids_never_reach_dynamodb() {
        assert!(matches!(
            handle(&app(), &request("[admin]", "x")).await,
            Err(ApiError::NotFound)
        ));
    }
}
