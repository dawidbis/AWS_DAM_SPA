//! Operacje S3 na uploadzie multipart wspólne dla Lambd uploadu.

use std::time::Duration;

use aws_sdk_s3::Client;
use aws_sdk_s3::config::RequestChecksumCalculation;
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
use serde::Serialize;

use crate::upload::StoredPart;

/// Klient S3 dla operacji uploadu. Sumy kontrolne tylko tam, gdzie S3 ich
/// wymaga: domyślne „when supported” dopisałoby do presigned URL-i części
/// sumę, której przeglądarka nie wysyła, i upload kończyłby się błędem.
#[must_use]
pub fn s3_client(config: &aws_sdk_s3::config::Config) -> Client {
    let config = config
        .to_builder()
        .request_checksum_calculation(RequestChecksumCalculation::WhenRequired)
        .build();
    Client::from_conf(config)
}

/// Ważność presigned URL do części (rozdział 4: krótko żyjące linki).
pub const PART_URL_TTL: Duration = Duration::from_hours(1);

/// Adres, pod który przeglądarka wysyła część pliku (HTTP PUT).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresignedPart {
    pub part_number: u64,
    pub url: String,
}

/// Część zapisana w S3 wraz z ETag (do CompleteMultipartUpload).
#[derive(Debug, Clone)]
pub struct ListedPart {
    pub stored: StoredPart,
    pub etag: String,
}

/// Wystawia presigned URL-e `UploadPart` dla wskazanych części. Podpis wiąże
/// URL z bucketem, kluczem, uploadId i numerem części (scenariusz 12).
///
/// # Errors
///
/// Błąd SDK przy podpisywaniu.
pub async fn presign_parts(
    s3: &Client,
    bucket: &str,
    key: &str,
    upload_id: &str,
    part_numbers: impl IntoIterator<Item = u64>,
) -> Result<Vec<PresignedPart>, String> {
    let mut parts = Vec::new();
    for part_number in part_numbers {
        let number = i32::try_from(part_number).map_err(|e| e.to_string())?;
        let config = PresigningConfig::expires_in(PART_URL_TTL).map_err(|e| e.to_string())?;
        let request = s3
            .upload_part()
            .bucket(bucket)
            .key(key)
            .upload_id(upload_id)
            .part_number(number)
            .presigned(config)
            .await
            .map_err(|e| format!("{e:?}"))?;
        parts.push(PresignedPart {
            part_number,
            url: request.uri().to_owned(),
        });
    }
    Ok(parts)
}

/// Lista części faktycznie zapisanych w S3 (ze stronicowaniem).
///
/// # Errors
///
/// Błąd SDK (np. upload przerwany lub nieistniejący).
pub async fn list_parts(
    s3: &Client,
    bucket: &str,
    key: &str,
    upload_id: &str,
) -> Result<Vec<ListedPart>, String> {
    let mut stream = s3
        .list_parts()
        .bucket(bucket)
        .key(key)
        .upload_id(upload_id)
        .into_paginator()
        .items()
        .send();
    let mut parts = Vec::new();
    while let Some(part) = stream.next().await {
        let part = part.map_err(|e| format!("{e:?}"))?;
        parts.push(ListedPart {
            stored: StoredPart {
                part_number: part
                    .part_number()
                    .and_then(|n| u64::try_from(n).ok())
                    .unwrap_or(0),
                size: part.size().and_then(|n| u64::try_from(n).ok()).unwrap_or(0),
            },
            etag: part.e_tag().unwrap_or_default().to_owned(),
        });
    }
    Ok(parts)
}

/// Kończy upload z częściami odczytanymi z S3 (nie z deklaracji klienta).
///
/// # Errors
///
/// Błąd SDK.
pub async fn complete(
    s3: &Client,
    bucket: &str,
    key: &str,
    upload_id: &str,
    parts: &[ListedPart],
) -> Result<(), String> {
    let mut completed: Vec<CompletedPart> = parts
        .iter()
        .map(|part| {
            CompletedPart::builder()
                .part_number(i32::try_from(part.stored.part_number).unwrap_or(i32::MAX))
                .e_tag(&part.etag)
                .build()
        })
        .collect();
    completed.sort_by_key(CompletedPart::part_number);
    s3.complete_multipart_upload()
        .bucket(bucket)
        .key(key)
        .upload_id(upload_id)
        .multipart_upload(
            CompletedMultipartUpload::builder()
                .set_parts(Some(completed))
                .build(),
        )
        .send()
        .await
        .map_err(|e| format!("{e:?}"))?;
    Ok(())
}

/// Przerywa upload i usuwa wysłane części.
///
/// # Errors
///
/// Błąd SDK.
pub async fn abort(s3: &Client, bucket: &str, key: &str, upload_id: &str) -> Result<(), String> {
    s3.abort_multipart_upload()
        .bucket(bucket)
        .key(key)
        .upload_id(upload_id)
        .send()
        .await
        .map_err(|e| format!("{e:?}"))?;
    Ok(())
}

/// Klucz obiektu w kwarantannie: wyłącznie identyfikator nadany przez serwer.
#[must_use]
pub fn quarantine_key(asset_id: &str) -> String {
    asset_id.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_s3::config::{BehaviorVersion, Credentials, Region};

    fn client() -> Client {
        let config = aws_sdk_s3::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new("eu-central-1"))
            .credentials_provider(Credentials::new("AKIDTEST", "secret", None, None, "test"))
            .build();
        s3_client(&config)
    }

    #[tokio::test]
    async fn presigned_part_url_is_bound_to_key_upload_and_part() {
        let parts = presign_parts(&client(), "quarantine", "asset-1", "upload-xyz", [1, 2])
            .await
            .unwrap();

        assert_eq!(parts.len(), 2);
        let url = &parts[1].url;
        assert!(
            url.starts_with("https://quarantine.s3.eu-central-1.amazonaws.com/asset-1?"),
            "{url}"
        );
        assert!(url.contains("partNumber=2"));
        assert!(url.contains("uploadId=upload-xyz"));
        assert!(url.contains("X-Amz-Expires=3600"));
        assert!(url.contains("X-Amz-Signature="));
        // Przeglądarka nie wysyła sumy kontrolnej, więc URL nie może jej wymagać.
        assert!(!url.to_ascii_lowercase().contains("checksum"), "{url}");
    }
}
