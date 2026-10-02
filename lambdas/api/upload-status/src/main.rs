//! `GET /uploads/{assetId}`: stan uploadu do wznowienia przerwanego transferu.
//!
//! Zwraca części już zapisane w S3 i świeże presigned URL-e dla brakujących
//! (stare mogły wygasnąć). Tylko autor uploadu widzi jego stan.

use std::sync::Arc;

use lambda_http::{Error, Request, http::StatusCode, service_fn};
use serde::Serialize;
use shared::assets::{StoreError, UploadSession, get_upload_session};
use shared::http::{self, ApiError};
use shared::multipart::{self, PART_URL_TTL, PresignedPart, quarantine_key};
use shared::{AssetStatus, Caller, UserGroup};

struct App {
    s3: aws_sdk_s3::Client,
    dynamo: aws_sdk_dynamodb::Client,
    table: String,
    bucket: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UploadStatusResponse {
    asset_id: String,
    status: AssetStatus,
    part_size: u64,
    part_count: u64,
    uploaded_parts: Vec<u64>,
    parts: Vec<PresignedPart>,
    urls_expire_in_seconds: u64,
}

const UPLOADERS: &[UserGroup] = &[UserGroup::Admin, UserGroup::Contributor];

/// Sesja uploadu należąca do wywołującego. Cudzy asset wygląda jak
/// nieistniejący, żeby nie zdradzać jego istnienia.
fn owned(session: UploadSession, caller: &Caller) -> Result<UploadSession, ApiError> {
    if session.uploader_id == caller.sub {
        Ok(session)
    } else {
        Err(ApiError::NotFound)
    }
}

async fn handle(app: &App, request: &Request) -> Result<(StatusCode, UploadStatusResponse), ApiError> {
    let caller = http::caller(request)?;
    caller.require_any_group(UPLOADERS)?;
    let asset_id = http::path_param(request, "assetId")?;

    let session = match get_upload_session(&app.dynamo, &app.table, &asset_id).await {
        Ok(session) => owned(session, &caller)?,
        Err(StoreError::NotFound) => return Err(ApiError::NotFound),
        Err(error) => return Err(ApiError::internal(error)),
    };
    if session.status != AssetStatus::Uploading {
        return Ok((
            StatusCode::OK,
            UploadStatusResponse {
                asset_id,
                status: session.status,
                part_size: session.part_size,
                part_count: session.part_count,
                uploaded_parts: (1..=session.part_count).collect(),
                parts: Vec::new(),
                urls_expire_in_seconds: 0,
            },
        ));
    }

    let key = quarantine_key(&asset_id);
    let listed = multipart::list_parts(&app.s3, &app.bucket, &key, &session.upload_id)
        .await
        .map_err(ApiError::internal)?;
    let uploaded: Vec<u64> = listed.iter().map(|part| part.stored.part_number).collect();
    let missing = (1..=session.part_count).filter(|number| !uploaded.contains(number));
    let parts = multipart::presign_parts(&app.s3, &app.bucket, &key, &session.upload_id, missing)
        .await
        .map_err(ApiError::internal)?;

    Ok((
        StatusCode::OK,
        UploadStatusResponse {
            asset_id,
            status: session.status,
            part_size: session.part_size,
            part_count: session.part_count,
            uploaded_parts: uploaded,
            parts,
            urls_expire_in_seconds: PART_URL_TTL.as_secs(),
        },
    ))
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

    fn session(uploader: &str) -> UploadSession {
        UploadSession {
            asset_id: "a1".to_owned(),
            status: AssetStatus::Uploading,
            uploader_id: uploader.to_owned(),
            upload_id: "mpu".to_owned(),
            declared_size: 10,
            part_size: 10,
            part_count: 1,
        }
    }

    fn caller(sub: &str) -> Caller {
        Caller {
            sub: sub.to_owned(),
            email: None,
            groups: vec![UserGroup::Contributor],
        }
    }

    #[test]
    fn owner_sees_own_upload() {
        assert!(owned(session("u1"), &caller("u1")).is_ok());
    }

    #[test]
    fn other_users_get_not_found() {
        assert!(matches!(
            owned(session("u1"), &caller("u2")),
            Err(ApiError::NotFound)
        ));
    }
}
