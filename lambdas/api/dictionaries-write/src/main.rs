//! Edycja słowników przez A (rola `dam-dictionaries-write`):
//!
//! - `PUT /dictionaries/{kind}/{id}`: utworzenie lub zastąpienie wpisu,
//! - `DELETE /dictionaries/{kind}/{id}`: usunięcie wpisu.
//!
//! Mecz odwołuje się do sezonu i rozgrywek, więc zapis meczu sprawdza, czy
//! istnieją, a sezonu ani rozgrywek używanych przez mecz nie da się usunąć
//! (409). Assety mogą odwoływać się do usuniętego zawodnika czy meczu:
//! frontend pokazuje wtedy identyfikator, a A może poprawić metadane.

use std::sync::Arc;

use aws_sdk_dynamodb::types::AttributeValue;
use lambda_http::http::Method;
use lambda_http::{Error, Request, http::StatusCode, service_fn};
use serde::Serialize;
use shared::dictionary::{DictionaryEntry, DictionaryKind, dictionary_key, is_slug};
use shared::http::{self, ApiError};
use shared::{Caller, UserGroup};

struct App {
    dynamo: aws_sdk_dynamodb::Client,
    table: String,
}

/// Odpowiedź po zapisie lub usunięciu: rodzaj i identyfikator wpisu.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EntryRef {
    kind: DictionaryKind,
    id: String,
}

fn target(request: &Request) -> Result<(DictionaryKind, String), ApiError> {
    let kind = http::path_param(request, "kind")?;
    let kind = DictionaryKind::parse(&kind).ok_or(ApiError::NotFound)?;
    let id = http::path_param(request, "id")?;
    if !is_slug(&id) {
        return Err(ApiError::BadRequest(
            "Identyfikator: małe litery, cyfry i myślniki".to_owned(),
        ));
    }
    Ok((kind, id))
}

async fn exists(app: &App, kind: DictionaryKind, id: &str) -> Result<bool, ApiError> {
    let output = app
        .dynamo
        .get_item()
        .table_name(&app.table)
        .set_key(Some(dictionary_key(kind, id)))
        .projection_expression("id")
        .consistent_read(true)
        .send()
        .await
        .map_err(|e| ApiError::internal(format!("{e:?}")))?;
    Ok(output.item.is_some())
}

/// Czy którykolwiek mecz odwołuje się do sezonu lub rozgrywek `id`.
async fn used_by_matches(app: &App, kind: DictionaryKind, id: &str) -> Result<bool, ApiError> {
    if !matches!(kind, DictionaryKind::Seasons | DictionaryKind::Competitions) {
        return Ok(false);
    }
    let mut pages = app
        .dynamo
        .query()
        .table_name(&app.table)
        .key_condition_expression("kind = :kind")
        .expression_attribute_values(
            ":kind",
            AttributeValue::S(DictionaryKind::Matches.partition().to_owned()),
        )
        .into_paginator()
        .items()
        .send();
    while let Some(item) = pages.next().await {
        let item = item.map_err(|e| ApiError::internal(format!("{e:?}")))?;
        if let Some(DictionaryEntry::Match(game)) = DictionaryEntry::from_item(&item)
            && (game.season_id == id || game.competition_id == id)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn put(app: &App, request: &Request, caller: &Caller) -> Result<(StatusCode, EntryRef), ApiError> {
    let (kind, id) = target(request)?;
    let body = http::json_body::<serde_json::Value>(request)?;
    let entry = DictionaryEntry::parse(kind, &id, &body).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    for (reference_kind, reference_id) in entry.references() {
        if !exists(app, reference_kind, reference_id).await? {
            return Err(ApiError::BadRequest(format!(
                "Nie ma wpisu {}/{reference_id}",
                reference_kind.segment()
            )));
        }
    }
    app.dynamo
        .put_item()
        .table_name(&app.table)
        .set_item(Some(entry.to_item()))
        .send()
        .await
        .map_err(|e| ApiError::internal(format!("{e:?}")))?;
    tracing::info!(kind = kind.segment(), id, caller = %caller.sub, "dictionary entry saved");
    Ok((StatusCode::OK, EntryRef { kind, id }))
}

async fn delete(app: &App, request: &Request, caller: &Caller) -> Result<(StatusCode, EntryRef), ApiError> {
    let (kind, id) = target(request)?;
    if used_by_matches(app, kind, &id).await? {
        return Err(ApiError::Conflict(
            "Wpis jest używany przez mecze; najpierw je zmień lub usuń".to_owned(),
        ));
    }
    let result = app
        .dynamo
        .delete_item()
        .table_name(&app.table)
        .set_key(Some(dictionary_key(kind, &id)))
        .condition_expression("attribute_exists(id)")
        .send()
        .await;
    match result {
        Ok(_) => {}
        Err(error)
            if error.as_service_error().is_some_and(
                aws_sdk_dynamodb::operation::delete_item::DeleteItemError::is_conditional_check_failed_exception,
            ) =>
        {
            return Err(ApiError::NotFound);
        }
        Err(error) => return Err(ApiError::internal(format!("{error:?}"))),
    }
    tracing::warn!(kind = kind.segment(), id, caller = %caller.sub, "dictionary entry deleted");
    Ok((StatusCode::OK, EntryRef { kind, id }))
}

async fn handle(app: &App, request: &Request) -> Result<(StatusCode, EntryRef), ApiError> {
    let caller = http::caller(request)?;
    caller.require_any_group(&[UserGroup::Admin])?;
    match *request.method() {
        Method::PUT => put(app, request, &caller).await,
        Method::DELETE => delete(app, request, &caller).await,
        _ => Err(ApiError::NotFound),
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    shared::telemetry::init();
    let config = aws_config::load_from_env().await;
    let app = Arc::new(App {
        dynamo: aws_sdk_dynamodb::Client::new(&config),
        table: http::env("DICTIONARIES_TABLE"),
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
    use lambda_http::{Body, RequestExt};
    use shared::http::testing::request_as;

    fn app() -> App {
        let config = aws_sdk_dynamodb::Config::builder()
            .behavior_version(aws_sdk_dynamodb::config::BehaviorVersion::latest())
            .region(aws_sdk_dynamodb::config::Region::new("eu-central-1"))
            .credentials_provider(aws_sdk_dynamodb::config::Credentials::for_tests())
            .build();
        App {
            dynamo: aws_sdk_dynamodb::Client::from_conf(config),
            table: "dictionaries".to_owned(),
        }
    }

    fn request(groups: &str, method: Method, kind: &str, id: &str, body: &str) -> Request {
        let mut request = request_as("u1", groups).with_path_parameters(HashMap::from([
            ("kind".to_owned(), kind.to_owned()),
            ("id".to_owned(), id.to_owned()),
        ]));
        *request.method_mut() = method;
        *request.body_mut() = Body::Text(body.to_owned());
        request
    }

    #[tokio::test]
    async fn only_admins_can_edit_dictionaries() {
        for groups in ["[staff]", "[contributor]", "[viewer]", "[]"] {
            let result = handle(
                &app(),
                &request(groups, Method::PUT, "players", "jan", r#"{"name":"Jan"}"#),
            )
            .await;
            assert!(matches!(result, Err(ApiError::Forbidden)), "{groups}");
        }
    }

    #[tokio::test]
    async fn invalid_requests_never_reach_dynamodb() {
        let cases = [
            ("users", "jan", r#"{"name":"Jan"}"#),
            ("players", "Jan Kowalski", r#"{"name":"Jan"}"#),
            ("players", "jan", r#"{"name":"<script>"}"#),
            ("players", "jan", r#"{"name":"Jan","number":0}"#),
            (
                "matches",
                "m1",
                r#"{"seasonId":"s","competitionId":"c","opponent":"X","date":"2025-02-30","home":true}"#,
            ),
        ];
        for (kind, id, body) in cases {
            let result = handle(&app(), &request("[admin]", Method::PUT, kind, id, body)).await;
            assert!(
                matches!(result, Err(ApiError::BadRequest(_) | ApiError::NotFound)),
                "{kind}/{id}: {result:?}"
            );
        }
    }
}
