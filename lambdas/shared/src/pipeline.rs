//! Kontrakty kroków Step Functions `scan-pipeline` (rozdział 3.2) i operacje
//! wspólne dla Lambd pipeline'u.
//!
//! Wejście i wyjście kroków zawiera wyłącznie referencje (ID assetu) i wyniki
//! (werdykt, sygnatura), nigdy treść pliku (rozdział 3.4).

use aws_sdk_s3::Client;
use aws_sdk_s3::types::MetadataDirective;
use serde::{Deserialize, Serialize};

use crate::catalog::is_asset_id;

/// Wynik skanu antywirusowego (wyjście Lambdy `scan`). Pole `verdict`
/// rozgałęzia maszynę stanów.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "verdict",
    rename_all = "SCREAMING_SNAKE_CASE",
    rename_all_fields = "camelCase"
)]
pub enum ScanOutcome {
    Clean {
        engine: String,
    },
    Infected {
        signature: String,
        engine: String,
    },
    /// Błąd, timeout albo niepełny skan: plik nie trafia dalej (fail closed).
    Failed {
        reason: String,
    },
}

/// Limity obrazów sprawdzane przed pełnym dekodowaniem (bomby
/// dekompresyjne, scenariusz 6) i egzekwowane ponownie przy dekodowaniu.
pub mod limits {
    /// Największy plik obrazu, który pipeline dekoduje w pamięci Lambdy
    /// (ten sam limit sprawdza `upload-init`).
    pub const MAX_IMAGE_BYTES: u64 = crate::upload::MAX_UPLOAD_BYTES;
    /// Najdłuższy bok obrazu w pikselach.
    pub const MAX_DIMENSION: u32 = 20_000;
    /// Największa liczba pikseli (100 MP ≈ 400 MB jako RGBA).
    pub const MAX_PIXELS: u64 = 100_000_000;
}

/// Wynik walidacji typu i wymiarów (wyjście Lambdy `validate`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "result",
    rename_all = "SCREAMING_SNAKE_CASE",
    rename_all_fields = "camelCase"
)]
pub enum ValidationOutcome {
    Valid {
        /// Typ ustalony z magic bytes, nie z deklaracji klienta.
        detected_type: String,
        width: u32,
        height: u32,
        size_bytes: u64,
    },
    Rejected {
        reason: String,
    },
}

/// Pola EXIF przepuszczane z oryginału (rozdział 3.3: whitelista). Zapisywane
/// jako metadane assetu po sanityzacji, nie osadzane z powrotem w pliku.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreservedMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copyright: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taken_at: Option<String>,
}

/// Wynik rekonstrukcji treści (wyjście Lambdy `cdr`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "result",
    rename_all = "SCREAMING_SNAKE_CASE",
    rename_all_fields = "camelCase"
)]
pub enum DisarmOutcome {
    /// Zrekonstruowany plik leży w `clean` pod `staging/<assetId>`.
    Clean {
        size_bytes: u64,
        sha256: String,
        metadata: PreservedMetadata,
    },
    /// Obrazu nie da się zdekodować i zakodować ponownie.
    Rejected { reason: String },
}

/// Klucz zrekonstruowanego pliku w `clean` przed finalizacją.
#[must_use]
pub fn staging_key(asset_id: &str) -> String {
    format!("staging/{asset_id}")
}

/// Wejście kroku pipeline'u: ID assetu i wyniki poprzednich kroków.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepInput {
    pub asset_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scan: Option<ScanOutcome>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation: Option<ValidationOutcome>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disarm: Option<DisarmOutcome>,
}

impl StepInput {
    #[must_use]
    pub fn new(asset_id: impl Into<String>) -> Self {
        Self {
            asset_id: asset_id.into(),
            ..Self::default()
        }
    }

    /// ID assetu po sprawdzeniu formatu. Wejście pochodzi z maszyny stanów,
    /// ale klucz S3 budujemy tylko z poprawnego UUID.
    ///
    /// # Errors
    ///
    /// Gdy ID nie jest UUID nadanym przez `upload-init`.
    pub fn checked_asset_id(&self) -> Result<&str, String> {
        if is_asset_id(&self.asset_id) {
            Ok(&self.asset_id)
        } else {
            Err("niepoprawny identyfikator assetu".to_owned())
        }
    }
}

/// Nazwa wykonania Step Functions. Pierwsze wykonanie nazywa się jak asset,
/// więc powtórzone zdarzenie S3 nie uruchomi drugiego (scenariusz 13).
/// Ponowienie przez admina dostaje nazwę z numerem próby.
#[must_use]
pub fn execution_name(asset_id: &str, retry_at_millis: Option<u64>) -> String {
    match retry_at_millis {
        None => asset_id.to_owned(),
        Some(at) => format!("{asset_id}-retry-{at}"),
    }
}

/// Atrybuty wyniku skanu zapisywane przy assecie.
#[must_use]
pub fn scan_attributes(
    verdict: &str,
    engine: &str,
    scanned_at: u64,
) -> Vec<(&'static str, aws_sdk_dynamodb::types::AttributeValue)> {
    use aws_sdk_dynamodb::types::AttributeValue;
    vec![
        ("scanVerdict", AttributeValue::S(verdict.to_owned())),
        ("scanEngine", AttributeValue::S(engine.to_owned())),
        ("scannedAt", AttributeValue::N(scanned_at.to_string())),
    ]
}

/// Obiekt w S3: bucket i klucz.
#[derive(Debug, Clone, Copy)]
pub struct Location<'a> {
    pub bucket: &'a str,
    pub key: &'a str,
}

/// Przenosi obiekt (kopia + usunięcie źródła), idempotentnie: gdy źródła już
/// nie ma, a cel istnieje, poprzednia próba się udała.
///
/// # Errors
///
/// Błąd S3 albo brak obiektu zarówno w źródle, jak i w celu.
pub async fn move_object(s3: &Client, from: Location<'_>, to: Location<'_>) -> Result<(), String> {
    let copied = s3
        .copy_object()
        .copy_source(format!("{}/{}", from.bucket, from.key))
        .bucket(to.bucket)
        .key(to.key)
        .metadata_directive(MetadataDirective::Copy)
        .send()
        .await;
    if let Err(error) = copied {
        let missing_source = error
            .as_service_error()
            .is_some_and(|e| e.meta().code() == Some("NoSuchKey"));
        if !(missing_source && object_exists(s3, to.bucket, to.key).await?) {
            return Err(format!("kopiowanie {} -> {}: {error:?}", from.bucket, to.bucket));
        }
        tracing::info!(key = to.key, bucket = to.bucket, "object already moved");
    }
    delete_object(s3, from).await
}

/// Usuwa obiekt (w S3 usunięcie nieistniejącego klucza też się udaje).
///
/// # Errors
///
/// Błąd S3.
pub async fn delete_object(s3: &Client, at: Location<'_>) -> Result<(), String> {
    s3.delete_object()
        .bucket(at.bucket)
        .key(at.key)
        .send()
        .await
        .map(|_| ())
        .map_err(|e| format!("usunięcie z {}: {e:?}", at.bucket))
}

/// Czy obiekt istnieje. `ListObjectsV2` zamiast `HeadObject`: polityki
/// bucketów `clean` i `infected` blokują odczyt obiektów rolom pipeline'u,
/// a lista kluczy wymaga tylko `s3:ListBucket`.
async fn object_exists(s3: &Client, bucket: &str, key: &str) -> Result<bool, String> {
    let listed = s3
        .list_objects_v2()
        .bucket(bucket)
        .prefix(key)
        .max_keys(1)
        .send()
        .await
        .map_err(|e| format!("lista {bucket}: {e:?}"))?;
    Ok(listed.contents().iter().any(|object| object.key() == Some(key)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "0b5e6a1c-2f3d-4e5f-8a9b-0c1d2e3f4a5b";

    #[test]
    fn scan_outcome_matches_state_machine_contract() {
        let infected = ScanOutcome::Infected {
            signature: "Eicar-Test-Signature".to_owned(),
            engine: "ClamAV 1.4".to_owned(),
        };
        assert_eq!(
            serde_json::to_value(&infected).unwrap(),
            serde_json::json!({ "verdict": "INFECTED", "signature": "Eicar-Test-Signature", "engine": "ClamAV 1.4" })
        );
        let failed: ScanOutcome = serde_json::from_str(r#"{"verdict":"FAILED","reason":"timeout"}"#).unwrap();
        assert_eq!(
            failed,
            ScanOutcome::Failed {
                reason: "timeout".to_owned()
            }
        );
    }

    #[test]
    fn step_input_reads_state_machine_payload() {
        let input: StepInput = serde_json::from_value(serde_json::json!({
            "assetId": ID,
            "marked": true,
            "scan": { "verdict": "CLEAN", "engine": "ClamAV" }
        }))
        .unwrap();
        assert_eq!(input.checked_asset_id(), Ok(ID));
        assert!(matches!(input.scan, Some(ScanOutcome::Clean { .. })));
    }

    #[test]
    fn rejects_asset_ids_that_are_not_uuids() {
        let input = StepInput::new("../clean/x");
        assert!(input.checked_asset_id().is_err());
    }

    #[test]
    fn validation_and_disarm_match_state_machine_contract() {
        let valid = ValidationOutcome::Valid {
            detected_type: "image/jpeg".to_owned(),
            width: 10,
            height: 20,
            size_bytes: 300,
        };
        assert_eq!(
            serde_json::to_value(&valid).unwrap(),
            serde_json::json!({ "result": "VALID", "detectedType": "image/jpeg", "width": 10, "height": 20, "sizeBytes": 300 })
        );
        let rejected: DisarmOutcome = serde_json::from_str(r#"{"result":"REJECTED","reason":"x"}"#).unwrap();
        assert_eq!(
            rejected,
            DisarmOutcome::Rejected {
                reason: "x".to_owned()
            }
        );
        assert_eq!(staging_key(ID), format!("staging/{ID}"));
    }

    #[test]
    fn first_execution_is_named_after_the_asset() {
        assert_eq!(execution_name(ID, None), ID);
        let retry = execution_name(ID, Some(1_700_000_000_000));
        assert_eq!(retry, format!("{ID}-retry-1700000000000"));
        assert!(retry.len() <= 80, "limit nazwy wykonania Step Functions");
    }
}
