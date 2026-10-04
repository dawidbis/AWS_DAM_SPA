//! `GET /dictionaries`: wszystkie słowniki klubu naraz (rola
//! `dam-dictionaries-read`).
//!
//! Słowniki są małe (dziesiątki wpisów), więc jeden `Scan` tabeli
//! `dictionaries` wystarcza i frontend trzyma je w pamięci do wyświetlania
//! nazw i list wyboru. Czytać może każda grupa A–D (filtry galerii, etykiety
//! na kafelkach), ale listę sponsorów dostaje tylko A: sponsor z grupy D nie
//! powinien widzieć, z kim jeszcze klub współpracuje.

use std::sync::Arc;

use lambda_http::{Error, Request, http::StatusCode, service_fn};
use shared::dictionary::{Dictionaries, DictionaryEntry};
use shared::http::{self, ApiError};
use shared::{Caller, UserGroup};

struct App {
    dynamo: aws_sdk_dynamodb::Client,
    table: String,
}

const READERS: &[UserGroup] = &[
    UserGroup::Admin,
    UserGroup::Staff,
    UserGroup::Contributor,
    UserGroup::Viewer,
];

/// Słowniki widoczne dla wywołującego.
fn visible_to(mut all: Dictionaries, caller: &Caller) -> Dictionaries {
    if !caller.has_any_group(&[UserGroup::Admin]) {
        all.sponsors.clear();
    }
    all
}

async fn load(app: &App) -> Result<Dictionaries, ApiError> {
    let mut pages = app
        .dynamo
        .scan()
        .table_name(&app.table)
        .into_paginator()
        .items()
        .send();
    let mut entries = Vec::new();
    while let Some(item) = pages.next().await {
        let item = item.map_err(|e| ApiError::internal(format!("{e:?}")))?;
        if let Some(entry) = DictionaryEntry::from_item(&item) {
            entries.push(entry);
        } else {
            tracing::warn!("skipping malformed dictionary item");
        }
    }
    Ok(Dictionaries::from_entries(entries))
}

async fn handle(app: &App, request: &Request) -> Result<(StatusCode, Dictionaries), ApiError> {
    let caller = http::caller(request)?;
    caller.require_any_group(READERS)?;
    let all = load(app).await?;
    Ok((StatusCode::OK, visible_to(all, &caller)))
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
    use super::*;
    use shared::dictionary::Sponsor;
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

    fn with_sponsor() -> Dictionaries {
        Dictionaries {
            sponsors: vec![Sponsor {
                id: "bank".to_owned(),
                name: "Bank".to_owned(),
            }],
            ..Dictionaries::default()
        }
    }

    #[test]
    fn only_admins_see_sponsors() {
        let caller = |groups: &str| http::caller(&request_as("u1", groups)).unwrap();
        assert_eq!(visible_to(with_sponsor(), &caller("[admin]")).sponsors.len(), 1);
        for groups in ["[staff]", "[contributor]", "[viewer]"] {
            assert!(
                visible_to(with_sponsor(), &caller(groups)).sponsors.is_empty(),
                "{groups}"
            );
        }
    }

    #[tokio::test]
    async fn users_without_a_group_are_forbidden() {
        assert!(matches!(
            handle(&app(), &request_as("u1", "[]")).await,
            Err(ApiError::Forbidden)
        ));
    }
}
