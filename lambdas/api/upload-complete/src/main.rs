//! `POST /uploads/{assetId}/complete`: kończy upload multipart.
//!
//! Rozmiar sprawdzamy na podstawie części zapisanych w S3 (ListParts), zanim
//! obiekt w ogóle powstanie. Niezgodność z deklaracją przerywa upload, więc
//! w kwarantannie nie zostaje nic (scenariusz 11). Udany upload przechodzi
//! warunkowo UPLOADING → QUARANTINED i czeka na skan.

use std::sync::Arc;

use aws_sdk_dynamodb::types::AttributeValue;
use lambda_http::{Error, Request, http::StatusCode, service_fn};
use serde::Serialize;
use shared::assets::{StoreError, get_upload_session, transition};
use shared::http::{self, ApiError};
use shared::multipart::{self, quarantine_key};
use shared::upload::{PartsCheck, StoredPart, check_parts};
use shared::{AssetStatus, UserGroup};

struct App {
    s3: aws_sdk_s3::Client,
    dynamo: aws_sdk_dynamodb::Client,
    table: String,
    bucket: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CompleteResponse {
    asset_id: String,
    status: AssetStatus,
}

const UPLOADERS: &[UserGroup] = &[UserGroup::Admin, UserGroup::Contributor];

fn store_error(error: StoreError) -> ApiError {
    match error {
        StoreError::NotFound => ApiError::NotFound,
        StoreError::InvalidTransition(_) => ApiError::Conflict("Upload nie jest już w toku".to_owned()),
        StoreError::Dynamo(detail) => ApiError::Internal(detail),
    }
}

async fn handle(app: &App, request: &Request) -> Result<(StatusCode, CompleteResponse), ApiError> {
    let caller = http::caller(request)?;
    caller.require_any_group(UPLOADERS)?;
    let asset_id = http::path_param(request, "assetId")?;

    let session = get_upload_session(&app.dynamo, &app.table, &asset_id)
        .await
        .map_err(store_error)?;
    if session.uploader_id != caller.sub {
        return Err(ApiError::NotFound);
    }
    match session.status {
        AssetStatus::Uploading => {}
        // Powtórzone wywołanie po sukcesie: ta sama odpowiedź (idempotencja).
        AssetStatus::Quarantined => {
            return Ok((
                StatusCode::OK,
                CompleteResponse {
                    asset_id,
                    status: AssetStatus::Quarantined,
                },
            ));
        }
        _ => return Err(ApiError::Conflict("Upload nie jest już w toku".to_owned())),
    }

    let key = quarantine_key(&asset_id);
    let listed = multipart::list_parts(&app.s3, &app.bucket, &key, &session.upload_id)
        .await
        .map_err(ApiError::internal)?;
    let stored: Vec<StoredPart> = listed.iter().map(|part| part.stored.clone()).collect();

    match check_parts(
        session.declared_size,
        session.part_size,
        session.part_count,
        &stored,
    ) {
        PartsCheck::Missing(missing) => Err(ApiError::Conflict(format!(
            "Brakuje {} części pliku; wznów upload",
            missing.len()
        ))),
        PartsCheck::SizeMismatch { actual_total } => {
            tracing::warn!(
                asset_id = %asset_id,
                sub = %caller.sub,
                declared = session.declared_size,
                actual = actual_total,
                "upload size mismatch, aborting"
            );
            multipart::abort(&app.s3, &app.bucket, &key, &session.upload_id)
                .await
                .map_err(ApiError::internal)?;
            transition(
                &app.dynamo,
                &app.table,
                &asset_id,
                AssetStatus::Rejected,
                &[("rejectReason", AttributeValue::S("SIZE_MISMATCH".to_owned()))],
            )
            .await
            .map_err(store_error)?;
            Err(ApiError::Unprocessable(
                "Rozmiar pliku nie zgadza się z deklaracją".to_owned(),
            ))
        }
        PartsCheck::Complete => {
            multipart::complete(&app.s3, &app.bucket, &key, &session.upload_id, &listed)
                .await
                .map_err(ApiError::internal)?;
            transition(
                &app.dynamo,
                &app.table,
                &asset_id,
                AssetStatus::Quarantined,
                &[("sizeBytes", AttributeValue::N(session.declared_size.to_string()))],
            )
            .await
            .map_err(store_error)?;
            tracing::info!(asset_id = %asset_id, sub = %caller.sub, size = session.declared_size, "upload completed");
            Ok((
                StatusCode::OK,
                CompleteResponse {
                    asset_id,
                    status: AssetStatus::Quarantined,
                },
            ))
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    shared::telemetry::init();
    let config = aws_config::load_from_env().await;
    let app = Arc::new(App {
        s3: multipart::s3_client(&aws_sdk_s3::config::Config::from(&config)),
        dynamo: aws_sdk_dynamodb::Client::new(&config),
        table: http::env("ASSETS_TABLE"),
        bucket: http::env("QUARANTINE_BUCKET"),
    });
    lambda_http::run(service_fn(move |request: Request| {
        let app = Arc::clone(&app);
        async move { Ok::<_, Error>(http::respond(handle(&app, &request).await)) }
    }))
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_errors_map_to_http_errors() {
        assert!(matches!(store_error(StoreError::NotFound), ApiError::NotFound));
        assert!(matches!(
            store_error(StoreError::InvalidTransition(AssetStatus::Rejected)),
            ApiError::Conflict(_)
        ));
        assert!(matches!(
            store_error(StoreError::Dynamo("x".to_owned())),
            ApiError::Internal(_)
        ));
    }
}
