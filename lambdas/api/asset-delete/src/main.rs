//! `DELETE /assets/{assetId}`: usunięcie assetu przez A (rola `dam-asset-delete`).
//!
//! Usuwa pliki assetu (wersja po CDR w `clean`, miniatura i podgląd
//! w `renditions`, pozostałości w kwarantannie i `clean/staging`), a potem
//! rekord z tabeli `assets`, warunkowo na statusie odczytanym na początku:
//! jeśli w międzyczasie pipeline lub publikacja zmieniły status, rekord
//! zostaje (409). Buckety mają wersjonowanie, więc usunięte pliki można
//! odzyskać przez 30 dni (lifecycle wersji nieaktualnych).
//!
//! Nie usuwamy assetów w trakcie pipeline'u ani zainfekowanych (dowód
//! incydentu), patrz [`shared::catalog::can_delete`].

use std::sync::Arc;

use aws_sdk_dynamodb::types::AttributeValue;
use lambda_http::{Error, Request, http::StatusCode, service_fn};
use shared::assets::{StoreError, asset_pk, get_status};
use shared::catalog::{AssetDeletedResponse, can_delete, is_asset_id};
use shared::http::{self, ApiError};
use shared::pipeline::{Location, delete_object, preview_key, staging_key, thumbnail_key};
use shared::{AssetStatus, UserGroup};

struct App {
    s3: aws_sdk_s3::Client,
    dynamo: aws_sdk_dynamodb::Client,
    table: String,
    quarantine: String,
    clean: String,
    renditions: String,
}

/// Obiekty S3 należące do assetu (usunięcie nieistniejącego klucza się udaje).
fn asset_objects<'a>(app: &'a App, asset_id: &str) -> Vec<(&'a str, String)> {
    vec![
        (app.clean.as_str(), asset_id.to_owned()),
        (app.clean.as_str(), staging_key(asset_id)),
        (app.renditions.as_str(), thumbnail_key(asset_id)),
        (app.renditions.as_str(), preview_key(asset_id)),
        (app.quarantine.as_str(), asset_id.to_owned()),
    ]
}

async fn delete_record(app: &App, asset_id: &str, status: AssetStatus) -> Result<(), ApiError> {
    let result = app
        .dynamo
        .delete_item()
        .table_name(&app.table)
        .key("pk", AttributeValue::S(asset_pk(asset_id)))
        .condition_expression("#status = :status")
        .expression_attribute_names("#status", "status")
        .expression_attribute_values(":status", AttributeValue::S(status.as_str().to_owned()))
        .send()
        .await;
    match result {
        Ok(_) => Ok(()),
        Err(error)
            if error.as_service_error().is_some_and(
                aws_sdk_dynamodb::operation::delete_item::DeleteItemError::is_conditional_check_failed_exception,
            ) =>
        {
            Err(ApiError::Conflict("Asset status changed, try again".to_owned()))
        }
        Err(error) => Err(ApiError::internal(format!("{error:?}"))),
    }
}

async fn handle(app: &App, request: &Request) -> Result<(StatusCode, AssetDeletedResponse), ApiError> {
    let caller = http::caller(request)?;
    caller.require_any_group(&[UserGroup::Admin])?;
    let asset_id = http::path_param(request, "assetId")?;
    if !is_asset_id(&asset_id) {
        return Err(ApiError::NotFound);
    }

    let status = match get_status(&app.dynamo, &app.table, &asset_id).await {
        Ok((status, _)) => status,
        Err(StoreError::NotFound) => return Err(ApiError::NotFound),
        Err(error) => return Err(ApiError::internal(error)),
    };
    if !can_delete(status) {
        return Err(ApiError::Conflict(format!(
            "Asset in status {status} cannot be deleted"
        )));
    }

    for (bucket, key) in asset_objects(app, &asset_id) {
        delete_object(&app.s3, Location { bucket, key: &key })
            .await
            .map_err(ApiError::internal)?;
    }
    delete_record(app, &asset_id, status).await?;
    tracing::warn!(asset_id, %status, caller = %caller.sub, "asset deleted");
    Ok((StatusCode::OK, AssetDeletedResponse { asset_id }))
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    shared::telemetry::init();
    let config = aws_config::load_from_env().await;
    let app = Arc::new(App {
        s3: aws_sdk_s3::Client::new(&config),
        dynamo: aws_sdk_dynamodb::Client::new(&config),
        table: http::env("ASSETS_TABLE"),
        quarantine: http::env("QUARANTINE_BUCKET"),
        clean: http::env("CLEAN_BUCKET"),
        renditions: http::env("RENDITIONS_BUCKET"),
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
        let s3 = aws_sdk_s3::Config::builder()
            .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
            .region(aws_sdk_s3::config::Region::new("eu-central-1"))
            .credentials_provider(aws_sdk_s3::config::Credentials::for_tests())
            .build();
        App {
            s3: aws_sdk_s3::Client::from_conf(s3),
            dynamo: aws_sdk_dynamodb::Client::from_conf(dynamo),
            table: "assets".to_owned(),
            quarantine: "quarantine".to_owned(),
            clean: "clean".to_owned(),
            renditions: "renditions".to_owned(),
        }
    }

    fn request(groups: &str, asset_id: &str) -> Request {
        request_as("u1", groups)
            .with_path_parameters(HashMap::from([("assetId".to_owned(), asset_id.to_owned())]))
    }

    #[tokio::test]
    async fn only_admins_can_delete() {
        for groups in ["[staff]", "[contributor]", "[viewer]", "[]"] {
            let result = handle(&app(), &request(groups, "0b5e6a1c-2f3d-4e5f-8a9b-0c1d2e3f4a5b")).await;
            assert!(matches!(result, Err(ApiError::Forbidden)), "{groups}");
        }
    }

    #[tokio::test]
    async fn malformed_ids_never_reach_aws() {
        assert!(matches!(
            handle(&app(), &request("[admin]", "../clean/x")).await,
            Err(ApiError::NotFound)
        ));
    }

    #[test]
    fn removes_every_copy_of_the_asset() {
        let app = app();
        let id = "0b5e6a1c-2f3d-4e5f-8a9b-0c1d2e3f4a5b";
        let objects = asset_objects(&app, id);
        assert!(objects.contains(&("clean", id.to_owned())));
        assert!(objects.contains(&("clean", format!("staging/{id}"))));
        assert!(objects.contains(&("renditions", format!("thumb/{id}.jpg"))));
        assert!(objects.contains(&("renditions", format!("preview/{id}.jpg"))));
        assert!(objects.contains(&("quarantine", id.to_owned())));
        assert!(!objects.iter().any(|(bucket, _)| *bucket == "infected"));
    }
}
