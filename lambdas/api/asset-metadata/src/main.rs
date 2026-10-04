//! `PUT /assets/{assetId}/metadata`: metadane assetu ustawiane przez A
//! (rola `dam-asset-metadata`, etap 3).
//!
//! 1. Ciało przez JSON Schema i normalizację (`shared::metadata`).
//! 2. Każda referencja (sezon, rozgrywki, mecz, zawodnicy) musi istnieć
//!    w tabeli `dictionaries` (`BatchGetItem`); mecz wyznacza sezon
//!    i rozgrywki.
//! 3. Warunkowy `UpdateItem` w `assets`: metadane można zmieniać tylko
//!    assetom, które przeszły pipeline (`CLEAN_DRAFT`, `PUBLISHED`,
//!    `ARCHIVED`). Pliki w kwarantannie, odrzucone i zainfekowane → 409.
//!
//! Żądanie zastępuje całość: pole pominięte albo `null` jest usuwane.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::sync::Arc;

use aws_sdk_dynamodb::types::{AttributeValue, KeysAndAttributes};
use lambda_http::{Error, Request, http::StatusCode, service_fn};
use shared::assets::{asset_pk, now_millis};
use shared::catalog::is_asset_id;
use shared::dictionary::{DictionaryEntry, DictionaryKind, Match, dictionary_key};
use shared::http::{self, ApiError};
use shared::metadata::{AssetMetadata, MetadataError, parse_metadata_request};
use shared::{AssetStatus, UserGroup};

struct App {
    dynamo: aws_sdk_dynamodb::Client,
    assets_table: String,
    dictionaries_table: String,
}

/// Statusy, w których metadane można edytować (plik przeszedł pipeline).
const EDITABLE: [AssetStatus; 3] = [
    AssetStatus::CleanDraft,
    AssetStatus::Published,
    AssetStatus::Archived,
];

/// Wpisy słowników o podanych kluczach (BatchGetItem, max 100 kluczy; schemat
/// ogranicza referencje do 33).
async fn fetch_entries(app: &App, keys: &[(DictionaryKind, &str)]) -> Result<Vec<DictionaryEntry>, ApiError> {
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    let unique: HashSet<(DictionaryKind, &str)> = keys.iter().copied().collect();
    let mut request = KeysAndAttributes::builder().consistent_read(true);
    for (kind, id) in unique {
        request = request.keys(dictionary_key(kind, id));
    }
    let request = request.build().map_err(|e| ApiError::internal(e.to_string()))?;
    let mut pending = HashMap::from([(app.dictionaries_table.clone(), request)]);
    let mut entries = Vec::new();
    // Nieprzetworzone klucze (throttling) ponawiamy, aż wszystko wróci.
    for _ in 0..5 {
        let output = app
            .dynamo
            .batch_get_item()
            .set_request_items(Some(pending))
            .send()
            .await
            .map_err(|e| ApiError::internal(format!("{e:?}")))?;
        if let Some(items) = output.responses().and_then(|r| r.get(&app.dictionaries_table)) {
            entries.extend(items.iter().filter_map(DictionaryEntry::from_item));
        }
        match output.unprocessed_keys {
            Some(rest) if !rest.is_empty() => pending = rest,
            _ => return Ok(entries),
        }
    }
    Err(ApiError::internal("BatchGetItem: unprocessed keys after retries"))
}

/// Sprawdza referencje i uzgadnia sezon/rozgrywki z meczem.
fn resolve(mut metadata: AssetMetadata, found: &[DictionaryEntry]) -> Result<AssetMetadata, MetadataError> {
    let present: HashSet<(DictionaryKind, &str)> = found.iter().map(|e| (e.kind(), e.id())).collect();
    if let Some((kind, id)) = metadata
        .references()
        .into_iter()
        .find(|reference| !present.contains(reference))
    {
        return Err(MetadataError::UnknownReference {
            kind,
            id: id.to_owned(),
        });
    }
    let game: Option<Match> = metadata.match_id.as_ref().and_then(|id| {
        found.iter().find_map(|entry| match entry {
            DictionaryEntry::Match(game) if game.id == *id => Some(game.clone()),
            _ => None,
        })
    });
    if let Some(game) = game {
        metadata.align_with_match(&game)?;
    }
    Ok(metadata)
}

/// Wyrażenie `SET … REMOVE …` i wartości dla zapisu metadanych.
struct Update {
    expression: String,
    names: HashMap<String, String>,
    values: HashMap<String, AttributeValue>,
}

fn build_update(metadata: &AssetMetadata, caller_sub: &str, now: u64) -> Update {
    let mut set = vec![
        "updatedAt = :now".to_owned(),
        "metadataUpdatedAt = :now".to_owned(),
        "metadataUpdatedBy = :by".to_owned(),
    ];
    let mut remove = Vec::new();
    let mut names = HashMap::from([("#status".to_owned(), "status".to_owned())]);
    let mut values = HashMap::from([
        (":now".to_owned(), AttributeValue::N(now.to_string())),
        (":by".to_owned(), AttributeValue::S(caller_sub.to_owned())),
    ]);
    for (i, status) in EDITABLE.iter().enumerate() {
        values.insert(format!(":s{i}"), AttributeValue::S(status.as_str().to_owned()));
    }
    let strings = [
        ("title", metadata.title.clone()),
        ("category", metadata.category.map(|c| c.as_str().to_owned())),
        ("seasonId", metadata.season_id.clone()),
        ("competitionId", metadata.competition_id.clone()),
        ("matchId", metadata.match_id.clone()),
    ];
    // Puste zbiory (SS) są w DynamoDB niedozwolone, więc pusta lista = REMOVE.
    let sets = [("playerIds", &metadata.player_ids), ("tags", &metadata.tags)];
    let mut index = 0;
    let mut placeholder = |name: &str, names: &mut HashMap<String, String>| {
        index += 1;
        names.insert(format!("#a{index}"), name.to_owned());
        index
    };
    for (name, value) in strings {
        let i = placeholder(name, &mut names);
        match value {
            Some(value) => {
                set.push(format!("#a{i} = :v{i}"));
                values.insert(format!(":v{i}"), AttributeValue::S(value));
            }
            None => remove.push(format!("#a{i}")),
        }
    }
    for (name, list) in sets {
        let i = placeholder(name, &mut names);
        if list.is_empty() {
            remove.push(format!("#a{i}"));
        } else {
            set.push(format!("#a{i} = :v{i}"));
            values.insert(format!(":v{i}"), AttributeValue::Ss(list.clone()));
        }
    }
    let mut expression = format!("SET {}", set.join(", "));
    if !remove.is_empty() {
        let _ = write!(expression, " REMOVE {}", remove.join(", "));
    }
    Update {
        expression,
        names,
        values,
    }
}

async fn handle(app: &App, request: &Request) -> Result<(StatusCode, AssetMetadata), ApiError> {
    let caller = http::caller(request)?;
    caller.require_any_group(&[UserGroup::Admin])?;
    let asset_id = http::path_param(request, "assetId")?;
    if !is_asset_id(&asset_id) {
        return Err(ApiError::NotFound);
    }
    let body = http::json_body::<serde_json::Value>(request)?;
    let metadata = parse_metadata_request(&body).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let found = fetch_entries(app, &metadata.references()).await?;
    let metadata = resolve(metadata, &found).map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let update = build_update(&metadata, &caller.sub, now_millis());
    let placeholders: Vec<String> = (0..EDITABLE.len()).map(|i| format!(":s{i}")).collect();
    let result = app
        .dynamo
        .update_item()
        .table_name(&app.assets_table)
        .key("pk", AttributeValue::S(asset_pk(&asset_id)))
        .condition_expression(format!("#status IN ({})", placeholders.join(", ")))
        .update_expression(update.expression)
        .set_expression_attribute_names(Some(update.names))
        .set_expression_attribute_values(Some(update.values))
        .send()
        .await;
    match result {
        Ok(_) => {}
        Err(error)
            if error.as_service_error().is_some_and(
                aws_sdk_dynamodb::operation::update_item::UpdateItemError::is_conditional_check_failed_exception,
            ) =>
        {
            return Err(ApiError::Conflict(
                "Metadane można zmieniać tylko plikom po skanie (szkice, opublikowane, zarchiwizowane)".to_owned(),
            ));
        }
        Err(error) => return Err(ApiError::internal(format!("{error:?}"))),
    }
    tracing::info!(asset_id, caller = %caller.sub, "asset metadata updated");
    Ok((StatusCode::OK, metadata))
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    shared::telemetry::init();
    let config = aws_config::load_from_env().await;
    let app = Arc::new(App {
        dynamo: aws_sdk_dynamodb::Client::new(&config),
        assets_table: http::env("ASSETS_TABLE"),
        dictionaries_table: http::env("DICTIONARIES_TABLE"),
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
    use shared::dictionary::Player;
    use shared::http::testing::request_as;
    use shared::metadata::AssetCategory;

    const ID: &str = "0b5e6a1c-2f3d-4e5f-8a9b-0c1d2e3f4a5b";

    fn app() -> App {
        let config = aws_sdk_dynamodb::Config::builder()
            .behavior_version(aws_sdk_dynamodb::config::BehaviorVersion::latest())
            .region(aws_sdk_dynamodb::config::Region::new("eu-central-1"))
            .credentials_provider(aws_sdk_dynamodb::config::Credentials::for_tests())
            .build();
        App {
            dynamo: aws_sdk_dynamodb::Client::from_conf(config),
            assets_table: "assets".to_owned(),
            dictionaries_table: "dictionaries".to_owned(),
        }
    }

    fn request(groups: &str, asset_id: &str, body: &str) -> Request {
        let mut request = request_as("u1", groups)
            .with_path_parameters(HashMap::from([("assetId".to_owned(), asset_id.to_owned())]));
        *request.body_mut() = Body::Text(body.to_owned());
        request
    }

    fn game() -> DictionaryEntry {
        DictionaryEntry::Match(Match {
            id: "m1".to_owned(),
            season_id: "2025-26".to_owned(),
            competition_id: "liga".to_owned(),
            opponent: "FC Rywal".to_owned(),
            date: "2025-09-14".to_owned(),
            home: true,
        })
    }

    fn player(id: &str) -> DictionaryEntry {
        DictionaryEntry::Player(Player {
            id: id.to_owned(),
            name: id.to_owned(),
            number: None,
            position: None,
            active: true,
        })
    }

    #[tokio::test]
    async fn only_admins_can_edit_metadata() {
        for groups in ["[staff]", "[contributor]", "[viewer]"] {
            let result = handle(&app(), &request(groups, ID, "{}")).await;
            assert!(matches!(result, Err(ApiError::Forbidden)), "{groups}");
        }
    }

    #[tokio::test]
    async fn invalid_requests_never_reach_aws() {
        for (asset_id, body) in [
            ("../x", "{}"),
            (ID, r#"{"title":"<script>"}"#),
            (ID, r#"{"matchId":"Mecz z Rywalem"}"#),
            (ID, r#"{"status":"PUBLISHED"}"#),
        ] {
            let result = handle(&app(), &request("[admin]", asset_id, body)).await;
            assert!(
                matches!(result, Err(ApiError::NotFound | ApiError::BadRequest(_))),
                "{asset_id} {body}: {result:?}"
            );
        }
    }

    #[test]
    fn unknown_players_are_rejected() {
        let metadata = AssetMetadata {
            player_ids: vec!["jan".to_owned(), "duch".to_owned()],
            ..AssetMetadata::default()
        };
        assert_eq!(
            resolve(metadata, &[player("jan")]),
            Err(MetadataError::UnknownReference {
                kind: DictionaryKind::Players,
                id: "duch".to_owned()
            })
        );
    }

    #[test]
    fn match_sets_season_and_competition() {
        let metadata = AssetMetadata {
            match_id: Some("m1".to_owned()),
            ..AssetMetadata::default()
        };
        let resolved = resolve(metadata, &[game()]).unwrap();
        assert_eq!(resolved.season_id.as_deref(), Some("2025-26"));
        assert_eq!(resolved.competition_id.as_deref(), Some("liga"));
    }

    #[test]
    fn update_sets_present_fields_and_removes_missing_ones() {
        let metadata = AssetMetadata {
            category: Some(AssetCategory::MatchPhoto),
            player_ids: vec!["jan".to_owned()],
            ..AssetMetadata::default()
        };
        let update = build_update(&metadata, "admin-sub", 42);
        let name_of = |value: &str| {
            update
                .names
                .iter()
                .find(|(_, v)| *v == value)
                .map(|(k, _)| k.clone())
                .unwrap()
        };
        let (set, remove) = update.expression.split_once(" REMOVE ").unwrap();
        assert!(set.contains(&format!("{} = ", name_of("category"))));
        assert!(set.contains(&format!("{} = ", name_of("playerIds"))));
        for removed in ["title", "seasonId", "competitionId", "matchId", "tags"] {
            assert!(remove.contains(&name_of(removed)), "{removed}");
        }
        assert!(
            update
                .values
                .values()
                .any(|v| v == &AttributeValue::Ss(vec!["jan".to_owned()]))
        );
        assert_eq!(update.values[":by"], AttributeValue::S("admin-sub".to_owned()));
    }
}
