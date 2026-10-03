//! `POST /uploads`: rozpoczyna upload multipart do kwarantanny.
//!
//! Plik nigdy nie przechodzi przez API (rozdział 3.4). Lambda zakłada upload
//! multipart pod kluczem nadanym przez serwer, zapisuje asset ze statusem
//! UPLOADING i zwraca presigned URL-e dla każdej części.

use std::sync::Arc;

use aws_sdk_dynamodb::types::AttributeValue;
use lambda_http::{Error, Request, http::StatusCode, service_fn};
use serde::Serialize;
use shared::assets::{asset_pk, now_millis};
use shared::http::{self, ApiError};
use shared::multipart::{self, PART_URL_TTL, PresignedPart, quarantine_key};
use shared::upload::InitUploadRequest;
use shared::{AssetStatus, UserGroup};
use uuid::Uuid;

struct App {
    s3: aws_sdk_s3::Client,
    dynamo: aws_sdk_dynamodb::Client,
    table: String,
    bucket: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InitUploadResponse {
    asset_id: String,
    part_size: u64,
    part_count: u64,
    parts: Vec<PresignedPart>,
    urls_expire_in_seconds: u64,
}

/// Grupy, które mogą wgrywać pliki (rozdział 4: A i C).
const UPLOADERS: &[UserGroup] = &[UserGroup::Admin, UserGroup::Contributor];

async fn handle(app: &App, request: &Request) -> Result<(StatusCode, InitUploadResponse), ApiError> {
    let caller = http::caller(request)?;
    caller.require_any_group(UPLOADERS)?;
    let spec = http::json_body::<InitUploadRequest>(request)?
        .validate()
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;

    let asset_id = Uuid::new_v4().to_string();
    let key = quarantine_key(&asset_id);

    // Typ z deklaracji klienta nie trafia do S3: niezweryfikowany plik nigdy
    // nie powinien być serwowany np. jako text/html.
    let upload = app
        .s3
        .create_multipart_upload()
        .bucket(&app.bucket)
        .key(&key)
        .content_type("application/octet-stream")
        .metadata("asset-id", &asset_id)
        .send()
        .await
        .map_err(|error| ApiError::internal(format!("{error:?}")))?;
    let upload_id = upload
        .upload_id()
        .ok_or_else(|| ApiError::internal("missing upload id"))?
        .to_owned();

    let now = AttributeValue::N(now_millis().to_string());
    let mut put = app
        .dynamo
        .put_item()
        .table_name(&app.table)
        .condition_expression("attribute_not_exists(pk)")
        .item("pk", AttributeValue::S(asset_pk(&asset_id)))
        .item("assetId", AttributeValue::S(asset_id.clone()))
        .item(
            "status",
            AttributeValue::S(AssetStatus::Uploading.as_str().to_owned()),
        )
        .item("uploaderId", AttributeValue::S(caller.sub.clone()))
        .item("originalFilename", AttributeValue::S(spec.filename.clone()))
        .item(
            "declaredContentType",
            AttributeValue::S(spec.content_type.clone()),
        )
        .item("declaredSize", AttributeValue::N(spec.size.to_string()))
        .item("uploadId", AttributeValue::S(upload_id.clone()))
        .item("partSize", AttributeValue::N(spec.part_size.to_string()))
        .item("partCount", AttributeValue::N(spec.part_count.to_string()))
        .item("createdAt", now.clone())
        .item("updatedAt", now);
    if let Some(title) = &spec.title {
        put = put.item("title", AttributeValue::S(title.clone()));
    }
    if let Some(ip) = http::source_ip(request) {
        put = put.item("uploaderIp", AttributeValue::S(ip));
    }
    if let Err(error) = put.send().await {
        // Bez rekordu w bazie upload nie ma właściciela: sprzątamy od razu.
        let _ = multipart::abort(&app.s3, &app.bucket, &key, &upload_id).await;
        return Err(ApiError::internal(format!("{error:?}")));
    }

    let parts = multipart::presign_parts(&app.s3, &app.bucket, &key, &upload_id, 1..=spec.part_count)
        .await
        .map_err(ApiError::internal)?;

    tracing::info!(asset_id = %asset_id, sub = %caller.sub, size = spec.size, parts = spec.part_count, "upload started");
    Ok((
        StatusCode::CREATED,
        InitUploadResponse {
            asset_id,
            part_size: spec.part_size,
            part_count: spec.part_count,
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
    use lambda_http::{Body, RequestExt};
    use shared::http::testing::request_as;

    fn app() -> App {
        let region = aws_sdk_s3::config::Region::new("eu-central-1");
        let s3 = aws_sdk_s3::Config::builder()
            .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
            .region(region.clone())
            .build();
        let dynamo = aws_sdk_dynamodb::Config::builder()
            .behavior_version(aws_sdk_dynamodb::config::BehaviorVersion::latest())
            .region(region)
            .build();
        App {
            s3: aws_sdk_s3::Client::from_conf(s3),
            dynamo: aws_sdk_dynamodb::Client::from_conf(dynamo),
            table: "assets".to_owned(),
            bucket: "quarantine".to_owned(),
        }
    }

    fn with_body(request: Request, body: &str) -> Request {
        let (parts, _) = request.into_parts();
        Request::from_parts(parts, Body::Text(body.to_owned()))
    }

    #[tokio::test]
    async fn viewer_cannot_upload() {
        let request = with_body(
            request_as("u1", "[viewer]"),
            r#"{"filename":"a.jpg","size":1,"contentType":"image/jpeg"}"#,
        );
        assert!(matches!(handle(&app(), &request).await, Err(ApiError::Forbidden)));
    }

    #[tokio::test]
    async fn staff_cannot_upload() {
        let request = with_body(
            request_as("u1", "[staff]"),
            r#"{"filename":"a.jpg","size":1,"contentType":"image/jpeg"}"#,
        );
        assert!(matches!(handle(&app(), &request).await, Err(ApiError::Forbidden)));
    }

    #[tokio::test]
    async fn rejects_svg_before_touching_aws() {
        let request = with_body(
            request_as("u1", "[contributor]"),
            r#"{"filename":"a.svg","size":1,"contentType":"image/svg+xml"}"#,
        );
        assert!(matches!(
            handle(&app(), &request).await,
            Err(ApiError::BadRequest(_))
        ));
    }

    #[tokio::test]
    async fn rejects_unknown_fields() {
        let request = with_body(
            request_as("u1", "[contributor]"),
            r#"{"filename":"a.jpg","size":1,"contentType":"image/jpeg","key":"../other"}"#,
        );
        assert!(matches!(
            handle(&app(), &request).await,
            Err(ApiError::BadRequest(_))
        ));
    }

    #[tokio::test]
    async fn request_without_token_is_unauthorized() {
        let request =
            Request::default().with_request_context(lambda_http::request::RequestContext::ApiGatewayV2(
                lambda_http::aws_lambda_events::apigw::ApiGatewayV2httpRequestContext::default(),
            ));
        assert!(matches!(
            handle(&app(), &request).await,
            Err(ApiError::Unauthorized)
        ));
    }
}
