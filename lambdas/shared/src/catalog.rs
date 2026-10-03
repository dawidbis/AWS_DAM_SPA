//! Katalog assetów: modele odpowiedzi API dla galerii, „moich zgłoszeń”
//! i kolejki publikacji oraz zasady widoczności statusów (rozdziały 4 i 5).
//!
//! Typy z `#[ts(export)]` są eksportowane do Angulara przez `ts-rs` przy
//! `cargo test` (katalog z `.cargo/config.toml`). CI sprawdza, że
//! wygenerowane pliki w repo są aktualne.

use std::collections::HashMap;

use aws_sdk_dynamodb::types::AttributeValue;
use serde::{Deserialize, Serialize};

use crate::{AssetStatus, Caller, UserGroup};

/// Widok listy assetów (`GET /assets?view=...`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum AssetView {
    /// Opublikowane materiały (A, B).
    Gallery,
    /// Własne zgłoszenia wywołującego (A, C).
    Mine,
    /// Czyste pliki czekające na publikację (A).
    Drafts,
    /// Pliki, których skan się nie powiódł (A): do ponowienia.
    Failed,
}

impl AssetView {
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "gallery" => Some(Self::Gallery),
            "mine" => Some(Self::Mine),
            "drafts" => Some(Self::Drafts),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }

    /// Grupy, które mogą otworzyć widok.
    #[must_use]
    pub const fn allowed_groups(self) -> &'static [UserGroup] {
        match self {
            Self::Gallery => &[UserGroup::Admin, UserGroup::Staff],
            Self::Mine => &[UserGroup::Admin, UserGroup::Contributor],
            Self::Drafts | Self::Failed => &[UserGroup::Admin],
        }
    }

    /// Czy lista zawiera podglądy (presigned URL do pliku z bucketu `clean`).
    /// Grupa C nie pobiera oryginałów, więc „moje zgłoszenia” są bez podglądu.
    /// Pliki po nieudanym skanie leżą w kwarantannie, której nigdy nie
    /// udostępniamy, więc też są bez podglądu.
    #[must_use]
    pub const fn with_previews(self) -> bool {
        matches!(self, Self::Gallery | Self::Drafts)
    }
}

/// Asset na liście.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AssetSummary {
    pub asset_id: String,
    pub status: AssetStatus,
    pub title: Option<String>,
    pub original_filename: String,
    pub content_type: String,
    #[cfg_attr(test, ts(type = "number"))]
    pub size_bytes: u64,
    /// Milisekundy od epoki.
    #[cfg_attr(test, ts(type = "number"))]
    pub created_at: u64,
    #[cfg_attr(test, ts(type = "number"))]
    pub updated_at: u64,
    /// Krótko żyjący URL do podglądu (tylko obrazy, tylko A i B).
    pub preview_url: Option<String>,
}

/// Odpowiedź `GET /assets`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AssetListResponse {
    pub items: Vec<AssetSummary>,
    /// Kursor następnej strony (`?cursor=`), `null` na ostatniej stronie.
    pub next_cursor: Option<String>,
}

/// Odpowiedź `GET /assets/{assetId}/download`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DownloadResponse {
    pub url: String,
    #[cfg_attr(test, ts(type = "number"))]
    pub expires_in_seconds: u64,
}

/// Odpowiedź operacji zmieniających status (`publish`, `rescan`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AssetStatusResponse {
    pub asset_id: String,
    pub status: AssetStatus,
}

/// Status widziany przez autora zgłoszenia, który nie jest adminem.
///
/// `INFECTED` i `SCAN_FAILED` widzi tylko A (rozdział 5): autor dowiaduje się
/// wyłącznie, że plik odrzucono, bez szczegółów o wykryciu. `ARCHIVED`
/// również jest tylko dla A, więc taki asset znika z listy autora.
#[must_use]
pub const fn status_for_uploader(status: AssetStatus) -> Option<AssetStatus> {
    match status {
        AssetStatus::Infected | AssetStatus::ScanFailed => Some(AssetStatus::Rejected),
        AssetStatus::Archived => None,
        other => Some(other),
    }
}

/// Czy wywołujący może pobrać oryginał assetu w danym statusie.
/// A: opublikowane i czekające na publikację, B: tylko opublikowane.
#[must_use]
pub fn can_download(caller: &Caller, status: AssetStatus) -> bool {
    match status {
        AssetStatus::Published => caller.has_any_group(&[UserGroup::Admin, UserGroup::Staff]),
        AssetStatus::CleanDraft => caller.has_any_group(&[UserGroup::Admin]),
        _ => false,
    }
}

/// Rekord z tabeli `assets` potrzebny do listy i pobierania.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetRecord {
    pub asset_id: String,
    pub status: AssetStatus,
    pub uploader_id: String,
    pub title: Option<String>,
    pub original_filename: String,
    pub content_type: String,
    pub size_bytes: u64,
    pub created_at: u64,
    pub updated_at: u64,
}

impl AssetRecord {
    /// Parsuje rekord DynamoDB. Niekompletne rekordy są pomijane (`None`):
    /// lista nie może się wysypać przez jeden uszkodzony wpis.
    #[must_use]
    pub fn from_item(item: &HashMap<String, AttributeValue>) -> Option<Self> {
        let string = |name: &str| item.get(name).and_then(|v| v.as_s().ok()).cloned();
        let number = |name: &str| {
            item.get(name)
                .and_then(|v| v.as_n().ok())
                .and_then(|n| n.parse::<u64>().ok())
        };
        let status = string("status")?;
        let created_at = number("createdAt")?;
        Some(Self {
            asset_id: string("assetId")?,
            status: AssetStatus::ALL.into_iter().find(|s| s.as_str() == status)?,
            uploader_id: string("uploaderId")?,
            title: string("title"),
            original_filename: string("originalFilename")?,
            // Po pipeline'ie typ i rozmiar ustalone przez serwer, wcześniej deklaracja.
            content_type: string("detectedType").or_else(|| string("declaredContentType"))?,
            size_bytes: number("sizeBytes").or_else(|| number("declaredSize"))?,
            created_at,
            updated_at: number("updatedAt").unwrap_or(created_at),
        })
    }

    /// Podsumowanie dla odpowiedzi API ze statusem już przefiltrowanym
    /// przez zasady widoczności.
    #[must_use]
    pub fn into_summary(self, status: AssetStatus, preview_url: Option<String>) -> AssetSummary {
        AssetSummary {
            asset_id: self.asset_id,
            status,
            title: self.title,
            original_filename: self.original_filename,
            content_type: self.content_type,
            size_bytes: self.size_bytes,
            created_at: self.created_at,
            updated_at: self.updated_at,
            preview_url,
        }
    }

    #[must_use]
    pub fn is_image(&self) -> bool {
        self.content_type.starts_with("image/")
    }
}

/// Czy identyfikator ma format UUID nadawany przez `upload-init`.
#[must_use]
pub fn is_asset_id(value: &str) -> bool {
    value.len() == 36
        && value.char_indices().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_digit() || ('a'..='f').contains(&c),
        })
}

/// Kursor stronicowania: pozycja ostatniego elementu w indeksie GSI
/// (`createdAt` i `assetId`). Wartość klucza partycji indeksu (status albo
/// autor) wynika z widoku, więc klient nie może jej podmienić kursorem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    pub created_at: u64,
    pub asset_id: String,
}

impl Cursor {
    #[must_use]
    pub fn encode(&self) -> String {
        format!("{}.{}", self.created_at, self.asset_id)
    }

    #[must_use]
    pub fn decode(value: &str) -> Option<Self> {
        let (created_at, asset_id) = value.split_once('.')?;
        if !is_asset_id(asset_id) || created_at.is_empty() || !created_at.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        Some(Self {
            created_at: created_at.parse().ok()?,
            asset_id: asset_id.to_owned(),
        })
    }
}

/// Bezpieczna nazwa pliku do nagłówka `Content-Disposition`: tylko znaki
/// ASCII bez cudzysłowów, ukośników i znaków sterujących.
#[must_use]
pub fn attachment_filename(original: &str, asset_id: &str) -> String {
    let cleaned: String = original
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_start_matches('.');
    if cleaned.is_empty() {
        asset_id.to_owned()
    } else {
        cleaned.chars().take(120).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "0b5e6a1c-2f3d-4e5f-8a9b-0c1d2e3f4a5b";

    fn caller(groups: &[UserGroup]) -> Caller {
        Caller {
            sub: "u1".to_owned(),
            email: None,
            groups: groups.to_vec(),
        }
    }

    fn item() -> HashMap<String, AttributeValue> {
        let s = |v: &str| AttributeValue::S(v.to_owned());
        let n = |v: &str| AttributeValue::N(v.to_owned());
        HashMap::from([
            ("assetId".to_owned(), s(ID)),
            ("status".to_owned(), s("PUBLISHED")),
            ("uploaderId".to_owned(), s("u1")),
            ("originalFilename".to_owned(), s("gol.jpg")),
            ("declaredContentType".to_owned(), s("image/jpeg")),
            ("declaredSize".to_owned(), n("1024")),
            ("createdAt".to_owned(), n("1700000000000")),
            ("updatedAt".to_owned(), n("1700000001000")),
        ])
    }

    #[test]
    fn parses_asset_record() {
        let record = AssetRecord::from_item(&item()).unwrap();
        assert_eq!(record.status, AssetStatus::Published);
        assert_eq!(record.size_bytes, 1024);
        assert_eq!(record.title, None);
        assert!(record.is_image());
    }

    #[test]
    fn prefers_facts_established_by_the_pipeline() {
        let mut processed = item();
        processed.insert(
            "detectedType".to_owned(),
            AttributeValue::S("image/png".to_owned()),
        );
        processed.insert("sizeBytes".to_owned(), AttributeValue::N("900".to_owned()));
        let record = AssetRecord::from_item(&processed).unwrap();
        assert_eq!(record.content_type, "image/png");
        assert_eq!(record.size_bytes, 900);
    }

    #[test]
    fn skips_incomplete_records() {
        let mut broken = item();
        broken.remove("originalFilename");
        assert!(AssetRecord::from_item(&broken).is_none());
        let mut unknown = item();
        unknown.insert("status".to_owned(), AttributeValue::S("HACKED".to_owned()));
        assert!(AssetRecord::from_item(&unknown).is_none());
    }

    #[test]
    fn uploader_never_sees_detection_details() {
        assert_eq!(
            status_for_uploader(AssetStatus::Infected),
            Some(AssetStatus::Rejected)
        );
        assert_eq!(
            status_for_uploader(AssetStatus::ScanFailed),
            Some(AssetStatus::Rejected)
        );
        assert_eq!(status_for_uploader(AssetStatus::Archived), None);
        assert_eq!(
            status_for_uploader(AssetStatus::CleanDraft),
            Some(AssetStatus::CleanDraft)
        );
    }

    #[test]
    fn download_rules_follow_role_matrix() {
        let admin = caller(&[UserGroup::Admin]);
        let staff = caller(&[UserGroup::Staff]);
        let contributor = caller(&[UserGroup::Contributor]);
        let viewer = caller(&[UserGroup::Viewer]);

        assert!(can_download(&admin, AssetStatus::Published));
        assert!(can_download(&admin, AssetStatus::CleanDraft));
        assert!(can_download(&staff, AssetStatus::Published));
        assert!(!can_download(&staff, AssetStatus::CleanDraft));
        assert!(!can_download(&contributor, AssetStatus::Published));
        assert!(!can_download(&viewer, AssetStatus::Published));
        for status in [
            AssetStatus::Infected,
            AssetStatus::ScanFailed,
            AssetStatus::Quarantined,
        ] {
            assert!(!can_download(&admin, status), "{status}");
        }
    }

    #[test]
    fn views_are_restricted_by_group() {
        assert_eq!(AssetView::parse("drafts"), Some(AssetView::Drafts));
        assert_eq!(AssetView::parse("all"), None);
        assert!(
            !AssetView::Gallery
                .allowed_groups()
                .contains(&UserGroup::Contributor)
        );
        assert!(!AssetView::Gallery.allowed_groups().contains(&UserGroup::Viewer));
        assert_eq!(AssetView::Drafts.allowed_groups(), &[UserGroup::Admin]);
        assert!(!AssetView::Mine.with_previews());
        assert!(!AssetView::Failed.with_previews());
        assert_eq!(AssetView::Failed.allowed_groups(), &[UserGroup::Admin]);
    }

    #[test]
    fn cursor_round_trips_and_rejects_garbage() {
        let cursor = Cursor {
            created_at: 1_700_000_000_000,
            asset_id: ID.to_owned(),
        };
        assert_eq!(Cursor::decode(&cursor.encode()), Some(cursor));
        for bad in [
            "",
            "x",
            "123",
            "abc.def",
            &format!("-1.{ID}"),
            &format!("1.{ID}x"),
            "1.ASSET#x",
        ] {
            assert_eq!(Cursor::decode(bad), None, "{bad}");
        }
    }

    #[test]
    fn validates_asset_ids() {
        assert!(is_asset_id(ID));
        assert!(!is_asset_id(&ID.to_uppercase()));
        assert!(!is_asset_id("../../etc/passwd"));
    }

    #[test]
    fn attachment_filename_is_header_safe() {
        assert_eq!(attachment_filename("gol 1.jpg", ID), "gol 1.jpg");
        assert_eq!(attachment_filename("a\"b\r\n.jpg", ID), "a_b__.jpg");
        assert_eq!(attachment_filename("zdjęcie.jpg", ID), "zdj_cie.jpg");
        assert_eq!(attachment_filename("...", ID), ID);
    }
}
