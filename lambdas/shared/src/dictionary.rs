//! Słowniki klubu (rozdział 7.4, etap 3): zawodnicy, sezony, rozgrywki, mecze
//! i sponsorzy. Metadane assetów odwołują się do nich przez identyfikatory
//! zamiast wolnego tekstu, dzięki czemu galerię da się filtrować, a literówka
//! w nazwisku nie tworzy „nowego” zawodnika.
//!
//! Tabela `dictionaries`: klucz partycji `kind` (np. `PLAYER`), klucz
//! sortowania `id` (slug nadany przez A, np. `jan-kowalski`), atrybut `data`
//! z wpisem jako JSON. Wpisy edytuje wyłącznie grupa A.

use std::collections::HashMap;

use aws_sdk_dynamodb::types::AttributeValue;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Najdłuższa nazwa wpisu (zawodnik, rozgrywki, przeciwnik, sponsor).
pub const MAX_NAME_CHARS: usize = 100;
/// Najdłuższy identyfikator (slug).
pub const MAX_ID_CHARS: usize = 64;

/// Rodzaj słownika. W ścieżce API liczba mnoga (`/dictionaries/players/…`),
/// w DynamoDB stała nazwa partycji (`PLAYER`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum DictionaryKind {
    Players,
    Seasons,
    Competitions,
    Matches,
    Sponsors,
}

impl DictionaryKind {
    pub const ALL: [Self; 5] = [
        Self::Players,
        Self::Seasons,
        Self::Competitions,
        Self::Matches,
        Self::Sponsors,
    ];

    /// Rodzaj z segmentu ścieżki API.
    #[must_use]
    pub fn parse(segment: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.segment() == segment)
    }

    #[must_use]
    pub const fn segment(self) -> &'static str {
        match self {
            Self::Players => "players",
            Self::Seasons => "seasons",
            Self::Competitions => "competitions",
            Self::Matches => "matches",
            Self::Sponsors => "sponsors",
        }
    }

    /// Wartość klucza partycji w tabeli `dictionaries`.
    #[must_use]
    pub const fn partition(self) -> &'static str {
        match self {
            Self::Players => "PLAYER",
            Self::Seasons => "SEASON",
            Self::Competitions => "COMPETITION",
            Self::Matches => "MATCH",
            Self::Sponsors => "SPONSOR",
        }
    }

    #[must_use]
    pub fn from_partition(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.partition() == value)
    }
}

/// Pozycja zawodnika.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Position {
    Goalkeeper,
    Defender,
    Midfielder,
    Forward,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Player {
    pub id: String,
    pub name: String,
    /// Numer na koszulce (1–99).
    #[serde(default)]
    pub number: Option<u8>,
    #[serde(default)]
    pub position: Option<Position>,
    /// Zawodnik w obecnej kadrze (byli zawodnicy zostają w słowniku, bo
    /// odwołują się do nich archiwalne zdjęcia).
    #[serde(default = "default_true")]
    pub active: bool,
}

/// Sezon, np. `id = "2025-26"`, `name = "2025/26"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Season {
    pub id: String,
    pub name: String,
}

/// Rozgrywki, np. liga, puchar, sparingi.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Competition {
    pub id: String,
    pub name: String,
}

/// Mecz w ramach sezonu i rozgrywek.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Match {
    pub id: String,
    pub season_id: String,
    pub competition_id: String,
    pub opponent: String,
    /// Data meczu `RRRR-MM-DD`.
    pub date: String,
    /// Mecz u siebie (`true`) czy na wyjeździe.
    pub home: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Sponsor {
    pub id: String,
    pub name: String,
}

const fn default_true() -> bool {
    true
}

/// Odpowiedź `GET /dictionaries`: wszystkie słowniki naraz (są małe), każda
/// lista posortowana do wyświetlenia. Sponsorów widzi tylko A.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Dictionaries {
    pub players: Vec<Player>,
    pub seasons: Vec<Season>,
    pub competitions: Vec<Competition>,
    pub matches: Vec<Match>,
    pub sponsors: Vec<Sponsor>,
}

impl Dictionaries {
    /// Sortowanie do wyświetlenia: zawodnicy po numerze, sezony od
    /// najnowszego, mecze od najnowszego, reszta alfabetycznie.
    pub fn sort(&mut self) {
        self.players.sort_by(|a, b| {
            (a.number.is_none(), a.number, &a.name).cmp(&(b.number.is_none(), b.number, &b.name))
        });
        self.seasons.sort_by(|a, b| b.id.cmp(&a.id));
        self.competitions.sort_by(|a, b| a.name.cmp(&b.name));
        self.matches
            .sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.id.cmp(&b.id)));
        self.sponsors.sort_by(|a, b| a.name.cmp(&b.name));
    }
}

/// Jeden wpis dowolnego słownika.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DictionaryEntry {
    Player(Player),
    Season(Season),
    Competition(Competition),
    Match(Match),
    Sponsor(Sponsor),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DictionaryError {
    #[error("Identyfikator: małe litery, cyfry i myślniki (maks. {MAX_ID_CHARS} znaków)")]
    InvalidId,
    #[error("Niepoprawne dane wpisu: {0}")]
    InvalidBody(String),
    #[error("Pole {0} jest puste albo za długie (maks. {MAX_NAME_CHARS} znaków)")]
    InvalidText(&'static str),
    #[error("Numer zawodnika musi być z zakresu 1–99")]
    InvalidNumber,
    #[error("Data musi mieć format RRRR-MM-DD")]
    InvalidDate,
}

/// Czy identyfikator jest poprawnym slugiem (`jan-kowalski`, `2025-26`).
#[must_use]
pub fn is_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_CHARS
        && !value.starts_with('-')
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Tekst do wyświetlenia: przycięty, bez znaków sterujących i nawiasów
/// kątowych, niepusty, do `MAX_NAME_CHARS` znaków.
fn clean_text(value: &str, field: &'static str) -> Result<String, DictionaryError> {
    let trimmed = value.trim();
    let valid = !trimmed.is_empty()
        && trimmed.chars().count() <= MAX_NAME_CHARS
        && !trimmed.chars().any(|c| c.is_control() || c == '<' || c == '>');
    if valid {
        Ok(trimmed.to_owned())
    } else {
        Err(DictionaryError::InvalidText(field))
    }
}

/// Data `RRRR-MM-DD` z poprawnym dniem miesiąca (lata przestępne).
#[must_use]
pub fn is_iso_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let number = |range: std::ops::Range<usize>| value.get(range).and_then(|s| s.parse::<u32>().ok());
    let (Some(year), Some(month), Some(day)) = (number(0..4), number(5..7), number(8..10)) else {
        return false;
    };
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1900..=2999).contains(&year) && (1..=days).contains(&day)
}

fn from_body<T: DeserializeOwned>(id: &str, body: &serde_json::Value) -> Result<T, DictionaryError> {
    let mut object = body
        .as_object()
        .cloned()
        .ok_or_else(|| DictionaryError::InvalidBody("oczekiwano obiektu JSON".to_owned()))?;
    // Identyfikator pochodzi ze ścieżki, nie z ciała.
    if object.contains_key("id") {
        return Err(DictionaryError::InvalidBody(
            "pole id podaje się w ścieżce".to_owned(),
        ));
    }
    object.insert("id".to_owned(), serde_json::Value::String(id.to_owned()));
    serde_json::from_value(serde_json::Value::Object(object))
        .map_err(|error| DictionaryError::InvalidBody(error.to_string()))
}

impl DictionaryEntry {
    /// Wpis z żądania `PUT /dictionaries/{kind}/{id}` po walidacji.
    ///
    /// # Errors
    ///
    /// [`DictionaryError`] dla niepoprawnego identyfikatora, nieznanych pól,
    /// pustych lub za długich nazw, numeru spoza 1–99 albo złej daty.
    pub fn parse(kind: DictionaryKind, id: &str, body: &serde_json::Value) -> Result<Self, DictionaryError> {
        if !is_slug(id) {
            return Err(DictionaryError::InvalidId);
        }
        let entry = match kind {
            DictionaryKind::Players => {
                let mut player: Player = from_body(id, body)?;
                player.name = clean_text(&player.name, "name")?;
                if player.number.is_some_and(|n| !(1..=99).contains(&n)) {
                    return Err(DictionaryError::InvalidNumber);
                }
                Self::Player(player)
            }
            DictionaryKind::Seasons => {
                let mut season: Season = from_body(id, body)?;
                season.name = clean_text(&season.name, "name")?;
                Self::Season(season)
            }
            DictionaryKind::Competitions => {
                let mut competition: Competition = from_body(id, body)?;
                competition.name = clean_text(&competition.name, "name")?;
                Self::Competition(competition)
            }
            DictionaryKind::Matches => {
                let mut game: Match = from_body(id, body)?;
                game.opponent = clean_text(&game.opponent, "opponent")?;
                if !is_slug(&game.season_id) || !is_slug(&game.competition_id) {
                    return Err(DictionaryError::InvalidId);
                }
                if !is_iso_date(&game.date) {
                    return Err(DictionaryError::InvalidDate);
                }
                Self::Match(game)
            }
            DictionaryKind::Sponsors => {
                let mut sponsor: Sponsor = from_body(id, body)?;
                sponsor.name = clean_text(&sponsor.name, "name")?;
                Self::Sponsor(sponsor)
            }
        };
        Ok(entry)
    }

    #[must_use]
    pub const fn kind(&self) -> DictionaryKind {
        match self {
            Self::Player(_) => DictionaryKind::Players,
            Self::Season(_) => DictionaryKind::Seasons,
            Self::Competition(_) => DictionaryKind::Competitions,
            Self::Match(_) => DictionaryKind::Matches,
            Self::Sponsor(_) => DictionaryKind::Sponsors,
        }
    }

    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::Player(entry) => &entry.id,
            Self::Season(entry) => &entry.id,
            Self::Competition(entry) => &entry.id,
            Self::Match(entry) => &entry.id,
            Self::Sponsor(entry) => &entry.id,
        }
    }

    /// Wpisy, które muszą istnieć, zanim zapiszemy ten (mecz → sezon, rozgrywki).
    #[must_use]
    pub fn references(&self) -> Vec<(DictionaryKind, &str)> {
        match self {
            Self::Match(game) => vec![
                (DictionaryKind::Seasons, game.season_id.as_str()),
                (DictionaryKind::Competitions, game.competition_id.as_str()),
            ],
            _ => Vec::new(),
        }
    }

    /// Wpis jako JSON (atrybut `data`).
    #[must_use]
    pub fn to_json(&self) -> String {
        let value = match self {
            Self::Player(entry) => serde_json::to_string(entry),
            Self::Season(entry) => serde_json::to_string(entry),
            Self::Competition(entry) => serde_json::to_string(entry),
            Self::Match(entry) => serde_json::to_string(entry),
            Self::Sponsor(entry) => serde_json::to_string(entry),
        };
        value.unwrap_or_default()
    }

    /// Rekord DynamoDB.
    #[must_use]
    pub fn to_item(&self) -> HashMap<String, AttributeValue> {
        HashMap::from([
            (
                "kind".to_owned(),
                AttributeValue::S(self.kind().partition().to_owned()),
            ),
            ("id".to_owned(), AttributeValue::S(self.id().to_owned())),
            ("data".to_owned(), AttributeValue::S(self.to_json())),
        ])
    }

    /// Wpis z rekordu DynamoDB; uszkodzone rekordy są pomijane (`None`).
    #[must_use]
    pub fn from_item(item: &HashMap<String, AttributeValue>) -> Option<Self> {
        let kind = DictionaryKind::from_partition(item.get("kind")?.as_s().ok()?)?;
        let data = item.get("data")?.as_s().ok()?;
        let entry = match kind {
            DictionaryKind::Players => Self::Player(serde_json::from_str(data).ok()?),
            DictionaryKind::Seasons => Self::Season(serde_json::from_str(data).ok()?),
            DictionaryKind::Competitions => Self::Competition(serde_json::from_str(data).ok()?),
            DictionaryKind::Matches => Self::Match(serde_json::from_str(data).ok()?),
            DictionaryKind::Sponsors => Self::Sponsor(serde_json::from_str(data).ok()?),
        };
        Some(entry)
    }
}

impl Dictionaries {
    /// Składa odpowiedź z rekordów tabeli.
    #[must_use]
    pub fn from_entries(entries: impl IntoIterator<Item = DictionaryEntry>) -> Self {
        let mut all = Self::default();
        for entry in entries {
            match entry {
                DictionaryEntry::Player(entry) => all.players.push(entry),
                DictionaryEntry::Season(entry) => all.seasons.push(entry),
                DictionaryEntry::Competition(entry) => all.competitions.push(entry),
                DictionaryEntry::Match(entry) => all.matches.push(entry),
                DictionaryEntry::Sponsor(entry) => all.sponsors.push(entry),
            }
        }
        all.sort();
        all
    }
}

/// Klucz rekordu słownika.
#[must_use]
pub fn dictionary_key(kind: DictionaryKind, id: &str) -> HashMap<String, AttributeValue> {
    HashMap::from([
        ("kind".to_owned(), AttributeValue::S(kind.partition().to_owned())),
        ("id".to_owned(), AttributeValue::S(id.to_owned())),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn kinds_round_trip_between_path_and_partition() {
        for kind in DictionaryKind::ALL {
            assert_eq!(DictionaryKind::parse(kind.segment()), Some(kind));
            assert_eq!(DictionaryKind::from_partition(kind.partition()), Some(kind));
        }
        assert_eq!(DictionaryKind::parse("PLAYER"), None);
        assert_eq!(DictionaryKind::parse("users"), None);
    }

    #[test]
    fn slugs_are_lowercase_ascii() {
        for valid in ["jan-kowalski", "2025-26", "liga", "a"] {
            assert!(is_slug(valid), "{valid}");
        }
        for invalid in ["", "Jan", "jan kowalski", "-x", "../x", "ż", &"a".repeat(65)] {
            assert!(!is_slug(invalid), "{invalid}");
        }
    }

    #[test]
    fn dates_are_validated_with_leap_years() {
        assert!(is_iso_date("2025-09-14"));
        assert!(is_iso_date("2024-02-29"));
        assert!(!is_iso_date("2025-02-29"));
        assert!(!is_iso_date("2025-13-01"));
        assert!(!is_iso_date("2025-9-14"));
        assert!(!is_iso_date("14.09.2025"));
    }

    #[test]
    fn parses_a_player_with_id_from_path() {
        let entry = DictionaryEntry::parse(
            DictionaryKind::Players,
            "jan-kowalski",
            &json!({ "name": "  Jan Kowalski ", "number": 9, "position": "FORWARD" }),
        )
        .unwrap();
        assert_eq!(
            entry,
            DictionaryEntry::Player(Player {
                id: "jan-kowalski".to_owned(),
                name: "Jan Kowalski".to_owned(),
                number: Some(9),
                position: Some(Position::Forward),
                active: true,
            })
        );
    }

    #[test]
    fn rejects_invalid_entries() {
        let cases = [
            (DictionaryKind::Players, "Jan", json!({ "name": "Jan" })),
            (DictionaryKind::Players, "jan", json!({ "name": "<script>" })),
            (DictionaryKind::Players, "jan", json!({ "name": "" })),
            (
                DictionaryKind::Players,
                "jan",
                json!({ "name": "Jan", "number": 100 }),
            ),
            (
                DictionaryKind::Players,
                "jan",
                json!({ "name": "Jan", "salary": 1 }),
            ),
            (
                DictionaryKind::Players,
                "jan",
                json!({ "id": "other", "name": "Jan" }),
            ),
            (DictionaryKind::Seasons, "2025-26", json!("2025/26")),
            (
                DictionaryKind::Matches,
                "m1",
                json!({ "seasonId": "2025-26", "competitionId": "liga", "opponent": "Rywal", "date": "2025-02-30", "home": true }),
            ),
            (
                DictionaryKind::Matches,
                "m1",
                json!({ "seasonId": "../x", "competitionId": "liga", "opponent": "Rywal", "date": "2025-02-01", "home": true }),
            ),
        ];
        for (kind, id, body) in cases {
            assert!(DictionaryEntry::parse(kind, id, &body).is_err(), "{id} {body}");
        }
    }

    #[test]
    fn matches_reference_their_season_and_competition() {
        let entry = DictionaryEntry::parse(
            DictionaryKind::Matches,
            "2025-09-14-rywal",
            &json!({ "seasonId": "2025-26", "competitionId": "liga", "opponent": "FC Rywal", "date": "2025-09-14", "home": true }),
        )
        .unwrap();
        assert_eq!(
            entry.references(),
            vec![
                (DictionaryKind::Seasons, "2025-26"),
                (DictionaryKind::Competitions, "liga")
            ]
        );
    }

    #[test]
    fn items_round_trip_through_dynamodb_format() {
        let entry = DictionaryEntry::parse(
            DictionaryKind::Sponsors,
            "bank",
            &json!({ "name": "Bank Matchday" }),
        )
        .unwrap();
        assert_eq!(DictionaryEntry::from_item(&entry.to_item()), Some(entry));
    }

    /// Seed z `scripts/seed/dictionaries.json` (wgrywany przy pierwszym
    /// deployu) musi przejść tę samą walidację co `PUT /dictionaries`,
    /// a mecze muszą wskazywać istniejące sezony i rozgrywki.
    #[test]
    fn seed_file_contains_valid_entries() {
        let seed: serde_json::Value =
            serde_json::from_str(include_str!("../../../scripts/seed/dictionaries.json")).unwrap();
        let mut entries = Vec::new();
        for kind in DictionaryKind::ALL {
            for raw in seed[kind.segment()].as_array().unwrap() {
                let mut body = raw.as_object().unwrap().clone();
                let id = body.remove("id").unwrap();
                let entry =
                    DictionaryEntry::parse(kind, id.as_str().unwrap(), &serde_json::Value::Object(body))
                        .unwrap_or_else(|e| panic!("{raw}: {e}"));
                // Skrypt zapisuje surowy wpis jako `data`, więc musi on dać się
                // odczytać tak, jak robi to Lambda.
                let item = HashMap::from([
                    ("kind".to_owned(), AttributeValue::S(kind.partition().to_owned())),
                    ("id".to_owned(), AttributeValue::S(entry.id().to_owned())),
                    ("data".to_owned(), AttributeValue::S(raw.to_string())),
                ]);
                assert_eq!(DictionaryEntry::from_item(&item).as_ref(), Some(&entry), "{raw}");
                entries.push(entry);
            }
        }
        let ids: std::collections::HashSet<_> =
            entries.iter().map(|e| (e.kind(), e.id().to_owned())).collect();
        for entry in &entries {
            for (kind, id) in entry.references() {
                assert!(ids.contains(&(kind, id.to_owned())), "{} → {id}", entry.id());
            }
        }
        assert!(entries.len() <= 25, "seed mieści się w jednym BatchWriteItem");
    }

    #[test]
    fn sorts_for_display() {
        let player = |id: &str, number: Option<u8>| Player {
            id: id.to_owned(),
            name: id.to_owned(),
            number,
            position: None,
            active: true,
        };
        let season = |id: &str| Season {
            id: id.to_owned(),
            name: id.to_owned(),
        };
        let all = Dictionaries::from_entries([
            DictionaryEntry::Player(player("bez-numeru", None)),
            DictionaryEntry::Player(player("dziewiatka", Some(9))),
            DictionaryEntry::Player(player("bramkarz", Some(1))),
            DictionaryEntry::Season(season("2024-25")),
            DictionaryEntry::Season(season("2025-26")),
        ]);
        let ids: Vec<_> = all.players.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["bramkarz", "dziewiatka", "bez-numeru"]);
        assert_eq!(all.seasons[0].id, "2025-26");
    }
}
