//! Metadane assetu (etap 3): kategoria, sezon, rozgrywki, mecz, zawodnicy,
//! tagi i tytuł. Ustawia je A po skanie (`PUT /assets/{assetId}/metadata`).
//!
//! Ciało żądania przechodzi przez JSON Schema (jak `POST /uploads`), a potem
//! normalizację: tagi małymi literami bez duplikatów, zawodnicy posortowani.
//! Istnienie wpisów słowników sprawdza Lambda `asset-metadata` w DynamoDB.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::dictionary::{DictionaryKind, Match};

/// Najdłuższy tytuł (ten sam limit co przy uploadzie).
pub const MAX_TITLE_CHARS: usize = crate::upload::MAX_TITLE_CHARS;
pub const MAX_PLAYERS: usize = 30;
pub const MAX_TAGS: usize = 10;
pub const MAX_TAG_CHARS: usize = 30;

/// Kategoria assetu (rozdział 7.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AssetCategory {
    MatchPhoto,
    TrainingPhoto,
    Video,
    BrandIdentity,
    SponsorMaterial,
    PressDocument,
}

impl AssetCategory {
    pub const ALL: [Self; 6] = [
        Self::MatchPhoto,
        Self::TrainingPhoto,
        Self::Video,
        Self::BrandIdentity,
        Self::SponsorMaterial,
        Self::PressDocument,
    ];

    /// Wartość zapisywana w DynamoDB (taka sama jak w JSON).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MatchPhoto => "MATCH_PHOTO",
            Self::TrainingPhoto => "TRAINING_PHOTO",
            Self::Video => "VIDEO",
            Self::BrandIdentity => "BRAND_IDENTITY",
            Self::SponsorMaterial => "SPONSOR_MATERIAL",
            Self::PressDocument => "PRESS_DOCUMENT",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|category| category.as_str() == value)
    }
}

/// Metadane assetu: ciało `PUT /assets/{assetId}/metadata` i odpowiedź.
/// Żądanie zastępuje całość: brak pola albo `null` usuwa wartość.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssetMetadata {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub category: Option<AssetCategory>,
    #[serde(default)]
    pub season_id: Option<String>,
    #[serde(default)]
    pub competition_id: Option<String>,
    #[serde(default)]
    pub match_id: Option<String>,
    #[serde(default)]
    pub player_ids: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MetadataError {
    #[error("Niepoprawne pole {0}")]
    Schema(String),
    #[error("Nie ma wpisu {}/{id} w słownikach", kind.segment())]
    UnknownReference { kind: DictionaryKind, id: String },
    #[error("Sezon i rozgrywki muszą zgadzać się z wybranym meczem")]
    MatchMismatch,
}

/// JSON Schema żądania (ten sam plik leży w repo jako dokumentacja).
pub const ASSET_METADATA_SCHEMA: &str = include_str!("../schemas/asset-metadata.schema.json");

static ASSET_METADATA_VALIDATOR: std::sync::LazyLock<jsonschema::Validator> =
    std::sync::LazyLock::new(|| {
        let schema =
            serde_json::from_str(ASSET_METADATA_SCHEMA).expect("schemat asset-metadata to poprawny JSON");
        jsonschema::validator_for(&schema).expect("schemat asset-metadata jest poprawny")
    });

/// Waliduje ciało schematem, deserializuje i normalizuje. Komunikat błędu
/// wskazuje pole, ale nie powtarza wartości od klienta.
///
/// # Errors
///
/// [`MetadataError::Schema`] ze ścieżką pierwszego niepoprawnego pola.
pub fn parse_metadata_request(body: &serde_json::Value) -> Result<AssetMetadata, MetadataError> {
    if let Some(error) = ASSET_METADATA_VALIDATOR.iter_errors(body).next() {
        let path = error.instance_path().to_string();
        return Err(MetadataError::Schema(if path.is_empty() {
            "/".to_owned()
        } else {
            path
        }));
    }
    let metadata: AssetMetadata =
        serde_json::from_value(body.clone()).map_err(|_| MetadataError::Schema("/".to_owned()))?;
    Ok(metadata.normalized())
}

impl AssetMetadata {
    /// Pusty tytuł jako brak, tagi małymi literami bez powtórzeń i spacji
    /// na brzegach, zawodnicy bez powtórzeń w stałej kolejności.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.title = self
            .title
            .map(|title| title.trim().to_owned())
            .filter(|title| !title.is_empty());
        let tags: BTreeSet<String> = self
            .tags
            .iter()
            .map(|tag| tag.trim().to_lowercase())
            .filter(|tag| !tag.is_empty())
            .collect();
        self.tags = tags.into_iter().collect();
        let players: BTreeSet<String> = self.player_ids.into_iter().collect();
        self.player_ids = players.into_iter().collect();
        self
    }

    /// Wpisy słowników, które muszą istnieć.
    #[must_use]
    pub fn references(&self) -> Vec<(DictionaryKind, &str)> {
        let mut references = Vec::new();
        if let Some(id) = &self.season_id {
            references.push((DictionaryKind::Seasons, id.as_str()));
        }
        if let Some(id) = &self.competition_id {
            references.push((DictionaryKind::Competitions, id.as_str()));
        }
        if let Some(id) = &self.match_id {
            references.push((DictionaryKind::Matches, id.as_str()));
        }
        references.extend(
            self.player_ids
                .iter()
                .map(|id| (DictionaryKind::Players, id.as_str())),
        );
        references
    }

    /// Mecz wyznacza sezon i rozgrywki: puste pola uzupełniamy z meczu, a
    /// sprzeczne odrzucamy (asset nie może być z meczu ligi i z pucharu).
    ///
    /// # Errors
    ///
    /// [`MetadataError::MatchMismatch`], gdy podany sezon lub rozgrywki
    /// różnią się od meczu.
    pub fn align_with_match(&mut self, game: &Match) -> Result<(), MetadataError> {
        let season_ok = self.season_id.as_ref().is_none_or(|id| *id == game.season_id);
        let competition_ok = self
            .competition_id
            .as_ref()
            .is_none_or(|id| *id == game.competition_id);
        if !season_ok || !competition_ok {
            return Err(MetadataError::MatchMismatch);
        }
        self.season_id = Some(game.season_id.clone());
        self.competition_id = Some(game.competition_id.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema() -> serde_json::Value {
        serde_json::from_str(ASSET_METADATA_SCHEMA).unwrap()
    }

    #[test]
    fn schema_limits_match_rust_constants() {
        let schema = schema();
        let props = &schema["properties"];
        assert_eq!(props["title"]["maxLength"], json!(MAX_TITLE_CHARS));
        assert_eq!(props["playerIds"]["maxItems"], json!(MAX_PLAYERS));
        assert_eq!(props["tags"]["maxItems"], json!(MAX_TAGS));
        assert_eq!(props["tags"]["items"]["maxLength"], json!(MAX_TAG_CHARS));
        let categories: Vec<_> = AssetCategory::ALL.iter().map(|c| json!(c.as_str())).collect();
        let mut expected = categories.clone();
        expected.push(serde_json::Value::Null);
        assert_eq!(props["category"]["enum"], json!(expected));
        assert_eq!(
            schema["$defs"]["slug"]["maxLength"],
            json!(crate::dictionary::MAX_ID_CHARS)
        );
    }

    #[test]
    fn accepts_and_normalizes_a_full_request() {
        let metadata = parse_metadata_request(&json!({
            "title": "  Gol w 90. minucie ",
            "category": "MATCH_PHOTO",
            "matchId": "2025-09-14-rywal",
            "playerIds": ["jan-kowalski", "adam-nowak"],
            "tags": ["Bramka", "bramka", "kibice", "Łódź 2025"]
        }))
        .unwrap();
        assert_eq!(metadata.title.as_deref(), Some("Gol w 90. minucie"));
        assert_eq!(metadata.category, Some(AssetCategory::MatchPhoto));
        assert_eq!(metadata.player_ids, ["adam-nowak", "jan-kowalski"]);
        assert_eq!(metadata.tags, ["bramka", "kibice", "łódź 2025"]);
    }

    #[test]
    fn empty_body_clears_everything() {
        assert_eq!(
            parse_metadata_request(&json!({})).unwrap(),
            AssetMetadata::default()
        );
        let cleared = parse_metadata_request(&json!({ "title": "  ", "category": null })).unwrap();
        assert_eq!(cleared, AssetMetadata::default());
    }

    #[test]
    fn rejects_scripts_unknown_fields_and_free_text_references() {
        for (body, field) in [
            (json!({ "title": "<script>alert(1)</script>" }), "/title"),
            (json!({ "uploaderId": "someone" }), "/"),
            (json!({ "category": "WALLPAPER" }), "/category"),
            (json!({ "matchId": "Mecz z Rywalem" }), "/matchId"),
            (json!({ "playerIds": ["jan", "jan"] }), "/playerIds"),
            (json!({ "playerIds": ["../admin"] }), "/playerIds/0"),
            (json!({ "tags": ["<b>"] }), "/tags/0"),
            (json!({ "tags": vec!["a"; 11] }), "/tags"),
            (json!([]), "/"),
        ] {
            assert_eq!(
                parse_metadata_request(&body),
                Err(MetadataError::Schema(field.to_owned())),
                "{body}"
            );
        }
    }

    #[test]
    fn lists_references_to_check() {
        let metadata = AssetMetadata {
            season_id: Some("2025-26".to_owned()),
            match_id: Some("m1".to_owned()),
            player_ids: vec!["jan".to_owned()],
            ..AssetMetadata::default()
        };
        assert_eq!(
            metadata.references(),
            vec![
                (DictionaryKind::Seasons, "2025-26"),
                (DictionaryKind::Matches, "m1"),
                (DictionaryKind::Players, "jan"),
            ]
        );
    }

    #[test]
    fn match_fills_in_season_and_competition() {
        let game = Match {
            id: "m1".to_owned(),
            season_id: "2025-26".to_owned(),
            competition_id: "liga".to_owned(),
            opponent: "FC Rywal".to_owned(),
            date: "2025-09-14".to_owned(),
            home: true,
        };
        let mut metadata = AssetMetadata {
            match_id: Some("m1".to_owned()),
            ..AssetMetadata::default()
        };
        metadata.align_with_match(&game).unwrap();
        assert_eq!(metadata.season_id.as_deref(), Some("2025-26"));
        assert_eq!(metadata.competition_id.as_deref(), Some("liga"));

        let mut conflicting = AssetMetadata {
            competition_id: Some("puchar".to_owned()),
            ..metadata
        };
        assert_eq!(
            conflicting.align_with_match(&game),
            Err(MetadataError::MatchMismatch)
        );
    }
}
