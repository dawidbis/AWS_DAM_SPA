//! `POST /assets/{assetId}/publish`: publikacja czystego assetu przez A
//! (`CLEAN_DRAFT` lub `ARCHIVED` → `PUBLISHED`, rola `dam-asset-publish`).
//!
//! Przejście to warunkowy zapis w DynamoDB, więc nie da się opublikować
//! assetu zainfekowanego, w kwarantannie ani nieistniejącego (rozdział 5).

use std::sync::Arc;

use aws_sdk_dynamodb::types::AttributeValue;
use lambda_http::{Error, Request, http::StatusCode, service_fn};
use shared::assets::{StoreError, now_millis, transition};
use shared::catalog::{PublishResponse, is_asset_id};
use shared::http::{self, ApiError};
use shared::{AssetStatus, UserGroup};

struct App {
    dynamo: aws_sdk_dynamodb::Client,
    table: String,
}

async fn handle(app: &App, request: &Request) -> Result<(StatusCode, PublishResponse), ApiError> {
    let caller = http::caller(request)?;
    caller.require_any_group(&[UserGroup::Admin])?;
    let asset_id = http::path_param(request, "assetId")?;
    if !is_asset_id(&asset_id) {
        return Err(ApiError::NotFound);
    }

    let extra = [
        ("publishedAt", AttributeValue::N(now_millis().to_string())),
        ("publishedBy", AttributeValue::S(caller.sub.clone())),
    ];
    match transition(&app.dynamo, &app.table, &asset_id, AssetStatus::Published, &extra).await {
        Ok(()) => {
            tracing::info!(asset_id, caller = %caller.sub, "asset published");
            Ok((
                StatusCode::OK,
                PublishResponse {
                    asset_id,
                    status: AssetStatus::Published,
                },
            ))
        }
        Err(StoreError::InvalidTransition(_) | StoreError::NotFound) => Err(ApiError::Conflict(
            "Asset cannot be published in its current status".to_owned(),
        )),
        Err(error) => Err(ApiError::internal(error)),
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    shared::telemetry::init();
    let config = aws_config::load_from_env().await;
    let app = Arc::new(App {
        dynamo: aws_sdk_dynamodb::Client::new(&config),
        table: http::env("ASSETS_TABLE"),
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
        let config = aws_sdk_dynamodb::Config::builder()
            .behavior_version(aws_sdk_dynamodb::config::BehaviorVersion::latest())
            .region(aws_sdk_dynamodb::config::Region::new("eu-central-1"))
            .credentials_provider(aws_sdk_dynamodb::config::Credentials::for_tests())
            .build();
        App {
            dynamo: aws_sdk_dynamodb::Client::from_conf(config),
            table: "assets".to_owned(),
        }
    }

    fn request(groups: &str, asset_id: &str) -> Request {
        request_as("u1", groups)
            .with_path_parameters(HashMap::from([("assetId".to_owned(), asset_id.to_owned())]))
    }

    #[tokio::test]
    async fn only_admins_can_publish() {
        for groups in ["[staff]", "[contributor]", "[viewer]", "[]"] {
            let result = handle(&app(), &request(groups, "0b5e6a1c-2f3d-4e5f-8a9b-0c1d2e3f4a5b")).await;
            assert!(matches!(result, Err(ApiError::Forbidden)), "{groups}");
        }
    }

    #[tokio::test]
    async fn malformed_ids_never_reach_dynamodb() {
        let result = handle(&app(), &request("[admin]", "ASSET#x")).await;
        assert!(matches!(result, Err(ApiError::NotFound)));
    }
}
