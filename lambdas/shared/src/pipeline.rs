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

/// Wejście kroku pipeline'u: ID assetu i (po skanie) jego wynik.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepInput {
    pub asset_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scan: Option<ScanOutcome>,
}

impl StepInput {
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

/// Przenosi obiekt między bucketami (kopia + usunięcie źródła), idempotentnie:
/// gdy źródła już nie ma, a cel istnieje, poprzednia próba się udała.
///
/// # Errors
///
/// Błąd S3 albo brak obiektu zarówno w źródle, jak i w celu.
pub async fn move_object(s3: &Client, from_bucket: &str, to_bucket: &str, key: &str) -> Result<(), String> {
    let copied = s3
        .copy_object()
        .copy_source(format!("{from_bucket}/{key}"))
        .bucket(to_bucket)
        .key(key)
        .metadata_directive(MetadataDirective::Copy)
        .send()
        .await;
    if let Err(error) = copied {
        let missing_source = error.as_service_error().is_some_and(|e| e.meta().code() == Some("NoSuchKey"));
        if !(missing_source && object_exists(s3, to_bucket, key).await?) {
            return Err(format!("kopiowanie {from_bucket} -> {to_bucket}: {error:?}"));
        }
        tracing::info!(key, to_bucket, "object already moved");
    }
    s3.delete_object()
        .bucket(from_bucket)
        .key(key)
        .send()
        .await
        .map_err(|e| format!("usunięcie z {from_bucket}: {e:?}"))?;
    Ok(())
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
        let input = StepInput {
            asset_id: "../clean/x".to_owned(),
            scan: None,
        };
        assert!(input.checked_asset_id().is_err());
    }

    #[test]
    fn first_execution_is_named_after_the_asset() {
        assert_eq!(execution_name(ID, None), ID);
        let retry = execution_name(ID, Some(1_700_000_000_000));
        assert_eq!(retry, format!("{ID}-retry-1700000000000"));
        assert!(retry.len() <= 80, "limit nazwy wykonania Step Functions");
    }
}
