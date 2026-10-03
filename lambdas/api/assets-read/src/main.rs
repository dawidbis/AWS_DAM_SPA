//! Odczyt katalogu assetów (rola `dam-assets-read`):
//!
//! - `GET /assets?view=gallery|mine|drafts|failed[&cursor=]`: galeria (A, B),
//!   własne zgłoszenia (A, C), kolejka publikacji i nieudane skany (A),
//!   Podglądy: miniatury z `renditions` dla A i B, podglądy ze znakiem
//!   wodnym dla D (nigdy oryginał),
//! - `GET /assets/{assetId}/download`: krótko żyjący presigned URL do
//!   oryginału z bucketu `clean` (A, B; rozdział 4).
//!
//! Grupy Cognito nie mają dostępu do S3. Link podpisuje ta funkcja po
//! sprawdzeniu uprawnień, a polityka bucketu `clean` pozwala czytać tylko
//! tej roli.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_s3::config::ResponseChecksumValidation;
use aws_sdk_s3::presigning::PresigningConfig;
use lambda_http::{Body, Error, Request, Response, http::StatusCode, service_fn};
use shared::assets::asset_pk;
use shared::catalog::{
    AssetListResponse, AssetRecord, AssetView, Cursor, DownloadResponse, PreviewSource, attachment_filename,
    can_download, is_asset_id, preview_source, status_for_uploader, watermark_only,
};
use shared::http::{self, ApiError};
use shared::pipeline::{preview_key, thumbnail_key};
use shared::{AssetStatus, Caller, UserGroup};

/// Ważność linków do plików (rozdział 4: np. 5 minut).
const FILE_URL_TTL: Duration = Duration::from_mins(5);
const PAGE_SIZE: i32 = 50;

struct App {
    s3: aws_sdk_s3::Client,
    dynamo: aws_sdk_dynamodb::Client,
    table: String,
    bucket: String,
    renditions: String,
}

/// Zapytanie do indeksu GSI wynikające z widoku i wywołującego.
#[derive(Debug, PartialEq, Eq)]
struct IndexQuery {
    index: &'static str,
    key_attribute: &'static str,
    key_value: String,
}

fn index_query(view: AssetView, caller: &Caller) -> IndexQuery {
    match view {
        AssetView::Gallery => status_query(AssetStatus::Published),
        AssetView::Drafts => status_query(AssetStatus::CleanDraft),
        AssetView::Failed => status_query(AssetStatus::ScanFailed),
        AssetView::Mine => IndexQuery {
            index: "uploader-index",
            key_attribute: "uploaderId",
            key_value: caller.sub.clone(),
        },
    }
}

fn status_query(status: AssetStatus) -> IndexQuery {
    IndexQuery {
        index: "status-index",
        key_attribute: "status",
        key_value: status.as_str().to_owned(),
    }
}

/// Status pokazywany na liście; `None` = asset niewidoczny dla wywołującego.
fn visible_status(view: AssetView, caller: &Caller, status: AssetStatus) -> Option<AssetStatus> {
    if view == AssetView::Mine && !caller.has_any_group(&[UserGroup::Admin]) {
        status_for_uploader(status)
    } else {
        Some(status)
    }
}

fn parse_view(request: &Request) -> Result<AssetView, ApiError> {
    let raw = http::query_param(request, "view").unwrap_or_else(|| "gallery".to_owned());
    AssetView::parse(&raw).ok_or_else(|| ApiError::BadRequest("Unknown view".to_owned()))
}

fn parse_cursor(request: &Request) -> Result<Option<Cursor>, ApiError> {
    http::query_param(request, "cursor")
        .map(|raw| Cursor::decode(&raw).ok_or_else(|| ApiError::BadRequest("Invalid cursor".to_owned())))
        .transpose()
}

async fn presign_get(
    app: &App,
    bucket: &str,
    key: &str,
    content_type: &str,
    disposition: String,
) -> Result<String, ApiError> {
    let config = PresigningConfig::expires_in(FILE_URL_TTL).map_err(ApiError::internal)?;
    let request = app
        .s3
        .get_object()
        .bucket(bucket)
        .key(key)
        .response_content_type(content_type)
        .response_content_disposition(disposition)
        .response_cache_control("private, no-store")
        .presigned(config)
        .await
        .map_err(|e| ApiError::internal(format!("{e:?}")))?;
    Ok(request.uri().to_owned())
}

async fn list(
    app: &App,
    request: &Request,
    caller: &Caller,
) -> Result<(StatusCode, AssetListResponse), ApiError> {
    let view = parse_view(request)?;
    caller.require_any_group(view.allowed_groups())?;
    let cursor = parse_cursor(request)?;
    let query = index_query(view, caller);

    let mut builder = app
        .dynamo
        .query()
        .table_name(&app.table)
        .index_name(query.index)
        .key_condition_expression("#key = :key")
        .expression_attribute_names("#key", query.key_attribute)
        .expression_attribute_values(":key", AttributeValue::S(query.key_value.clone()))
        .scan_index_forward(false)
        .limit(PAGE_SIZE);
    if let Some(cursor) = cursor {
        builder = builder.set_exclusive_start_key(Some(HashMap::from([
            ("pk".to_owned(), AttributeValue::S(asset_pk(&cursor.asset_id))),
            (query.key_attribute.to_owned(), AttributeValue::S(query.key_value)),
            (
                "createdAt".to_owned(),
                AttributeValue::N(cursor.created_at.to_string()),
            ),
        ])));
    }
    let output = builder
        .send()
        .await
        .map_err(|e| ApiError::internal(format!("{e:?}")))?;

    let mut items = Vec::new();
    for record in output.items().iter().filter_map(AssetRecord::from_item) {
        let Some(status) = visible_status(view, caller, record.status) else {
            continue;
        };
        let source = preview_source(view, caller, &record);
        // D widzi tylko assety z podglądem ze znakiem wodnym.
        if watermark_only(caller) && view == AssetView::Gallery && source != PreviewSource::Watermarked {
            continue;
        }
        let preview_url = match source {
            PreviewSource::Thumbnail => Some(
                presign_get(
                    app,
                    &app.renditions,
                    &thumbnail_key(&record.asset_id),
                    "image/jpeg",
                    "inline".to_owned(),
                )
                .await?,
            ),
            PreviewSource::Watermarked => Some(
                presign_get(
                    app,
                    &app.renditions,
                    &preview_key(&record.asset_id),
                    "image/jpeg",
                    "inline".to_owned(),
                )
                .await?,
            ),
            PreviewSource::Original => Some(
                presign_get(
                    app,
                    &app.bucket,
                    &record.asset_id,
                    &record.content_type,
                    "inline".to_owned(),
                )
                .await?,
            ),
            PreviewSource::None => None,
        };
        items.push(record.into_summary(status, preview_url));
    }

    let next_cursor = output.last_evaluated_key().and_then(|key| {
        let asset_id = key.get("pk")?.as_s().ok()?.strip_prefix("ASSET#")?.to_owned();
        let created_at = key.get("createdAt")?.as_n().ok()?.parse().ok()?;
        Some(Cursor { created_at, asset_id }.encode())
    });
    Ok((StatusCode::OK, AssetListResponse { items, next_cursor }))
}

async fn download(
    app: &App,
    caller: &Caller,
    asset_id: &str,
) -> Result<(StatusCode, DownloadResponse), ApiError> {
    caller.require_any_group(&[UserGroup::Admin, UserGroup::Staff])?;
    if !is_asset_id(asset_id) {
        return Err(ApiError::NotFound);
    }
    let output = app
        .dynamo
        .get_item()
        .table_name(&app.table)
        .key("pk", AttributeValue::S(asset_pk(asset_id)))
        .consistent_read(true)
        .send()
        .await
        .map_err(|e| ApiError::internal(format!("{e:?}")))?;
    let record = output
        .item()
        .and_then(AssetRecord::from_item)
        .ok_or(ApiError::NotFound)?;
    // Asset niedostępny dla wywołującego wygląda jak nieistniejący.
    if !can_download(caller, record.status) {
        return Err(ApiError::NotFound);
    }
    let filename = attachment_filename(&record.original_filename, asset_id);
    let url = presign_get(
        app,
        &app.bucket,
        asset_id,
        &record.content_type,
        format!("attachment; filename=\"{filename}\""),
    )
    .await?;
    tracing::info!(asset_id, caller = %caller.sub, "download url issued");
    Ok((
        StatusCode::OK,
        DownloadResponse {
            url,
            expires_in_seconds: FILE_URL_TTL.as_secs(),
        },
    ))
}

async fn handle(app: &App, request: &Request) -> Response<Body> {
    let caller = match http::caller(request) {
        Ok(caller) => caller,
        Err(error) => return error.into_response(),
    };
    match http::path_param(request, "assetId") {
        Ok(asset_id) => http::respond(download(app, &caller, &asset_id).await),
        Err(_) => http::respond(list(app, request, &caller).await),
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    shared::telemetry::init();
    let config = aws_config::load_from_env().await;
    // Bez walidacji sum kontrolnych odpowiedzi: inaczej SDK mógłby dopisać do
    // podpisu nagłówek, którego przeglądarka nie wyśle.
    let s3_config = aws_sdk_s3::config::Builder::from(&config)
        .response_checksum_validation(ResponseChecksumValidation::WhenRequired)
        .build();
    let app = Arc::new(App {
        s3: aws_sdk_s3::Client::from_conf(s3_config),
        dynamo: aws_sdk_dynamodb::Client::new(&config),
        table: http::env("ASSETS_TABLE"),
        bucket: http::env("CLEAN_BUCKET"),
        renditions: http::env("RENDITIONS_BUCKET"),
    });
    lambda_http::run(service_fn(move |request: Request| {
        let app = Arc::clone(&app);
        async move { Ok::<_, Error>(handle(&app, &request).await) }
    }))
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_s3::config::{Credentials, Region};
    use shared::http::testing::request_as;

    fn caller(groups: &[UserGroup]) -> Caller {
        Caller {
            sub: "u1".to_owned(),
            email: None,
            groups: groups.to_vec(),
        }
    }

    fn app() -> App {
        let s3_config = aws_sdk_s3::Config::builder()
            .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
            .region(Region::new("eu-central-1"))
            .credentials_provider(Credentials::for_tests())
            .response_checksum_validation(ResponseChecksumValidation::WhenRequired)
            .build();
        let dynamo_config = aws_sdk_dynamodb::Config::builder()
            .behavior_version(aws_sdk_dynamodb::config::BehaviorVersion::latest())
            .region(aws_sdk_dynamodb::config::Region::new("eu-central-1"))
            .credentials_provider(aws_sdk_dynamodb::config::Credentials::for_tests())
            .build();
        App {
            s3: aws_sdk_s3::Client::from_conf(s3_config),
            dynamo: aws_sdk_dynamodb::Client::from_conf(dynamo_config),
            table: "assets".to_owned(),
            bucket: "clean".to_owned(),
            renditions: "renditions".to_owned(),
        }
    }

    #[test]
    fn mine_queries_only_the_callers_own_assets() {
        let query = index_query(AssetView::Mine, &caller(&[UserGroup::Contributor]));
        assert_eq!(query.index, "uploader-index");
        assert_eq!(query.key_value, "u1");
        assert_eq!(
            index_query(AssetView::Gallery, &caller(&[])).key_value,
            "PUBLISHED"
        );
        assert_eq!(
            index_query(AssetView::Drafts, &caller(&[])).key_value,
            "CLEAN_DRAFT"
        );
    }

    #[test]
    fn contributors_see_infected_as_rejected_but_admins_see_the_truth() {
        let contributor = caller(&[UserGroup::Contributor]);
        let admin = caller(&[UserGroup::Admin]);
        assert_eq!(
            visible_status(AssetView::Mine, &contributor, AssetStatus::Infected),
            Some(AssetStatus::Rejected)
        );
        assert_eq!(
            visible_status(AssetView::Mine, &admin, AssetStatus::Infected),
            Some(AssetStatus::Infected)
        );
    }

    #[tokio::test]
    async fn contributor_cannot_open_gallery() {
        let mut request = request_as("u1", "[contributor]");
        request = lambda_http::RequestExt::with_query_string_parameters(
            request,
            HashMap::from([("view".to_owned(), "gallery".to_owned())]),
        );
        let response = handle(&app(), &request).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn unknown_view_and_bad_cursor_are_rejected_before_any_query() {
        for query in [("view", "everything"), ("cursor", "ASSET#x")] {
            let request = lambda_http::RequestExt::with_query_string_parameters(
                request_as("u1", "[admin]"),
                HashMap::from([(query.0.to_owned(), query.1.to_owned())]),
            );
            let response = handle(&app(), &request).await;
            assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{query:?}");
        }
    }

    #[tokio::test]
    async fn contributor_and_viewer_cannot_download() {
        for groups in ["[contributor]", "[viewer]"] {
            let request = lambda_http::RequestExt::with_path_parameters(
                request_as("u1", groups),
                HashMap::from([(
                    "assetId".to_owned(),
                    "0b5e6a1c-2f3d-4e5f-8a9b-0c1d2e3f4a5b".to_owned(),
                )]),
            );
            let response = handle(&app(), &request).await;
            assert_eq!(response.status(), StatusCode::FORBIDDEN, "{groups}");
        }
    }

    #[tokio::test]
    async fn download_url_is_short_lived_and_forces_attachment() {
        let url = presign_get(
            &app(),
            "clean",
            "asset-1",
            "image/jpeg",
            "attachment; filename=\"gol.jpg\"".to_owned(),
        )
        .await
        .unwrap();
        assert!(
            url.starts_with("https://clean.s3.eu-central-1.amazonaws.com/asset-1?"),
            "{url}"
        );
        assert!(url.contains("X-Amz-Expires=300"), "{url}");
        assert!(url.contains("response-content-disposition=attachment"), "{url}");
        // Przeglądarka wysyła tylko nagłówek Host: podpis nie może wymagać innych.
        assert!(
            url.contains("X-Amz-SignedHeaders=host&") || url.ends_with("X-Amz-SignedHeaders=host"),
            "{url}"
        );
    }
}
