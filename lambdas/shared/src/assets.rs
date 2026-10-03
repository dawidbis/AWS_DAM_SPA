//! Tabela `assets` w DynamoDB: zapis assetu i warunkowe przejścia statusów.
//!
//! Każda zmiana statusu to `UpdateItem` z `ConditionExpression` na obecnym
//! statusie (rozdział 5). Dzięki temu powtórzone zdarzenie nie zmieni stanu
//! drugi raz, a niedozwolone przejście (np. INFECTED → PUBLISHED) się nie uda.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::time::{SystemTime, UNIX_EPOCH};

use aws_sdk_dynamodb::Client;
use aws_sdk_dynamodb::types::AttributeValue;

use crate::AssetStatus;

/// Klucz partycji assetu.
#[must_use]
pub fn asset_pk(asset_id: &str) -> String {
    format!("ASSET#{asset_id}")
}

/// Bieżący czas w milisekundach od epoki (sortowanie w indeksach GSI).
#[must_use]
pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("asset nie istnieje")]
    NotFound,
    #[error("niedozwolone przejście statusu do {0}")]
    InvalidTransition(AssetStatus),
    #[error("DynamoDB: {0}")]
    Dynamo(String),
}

/// Stan uploadu multipart zapisany przy assecie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadSession {
    pub asset_id: String,
    pub status: AssetStatus,
    pub uploader_id: String,
    pub upload_id: String,
    pub declared_size: u64,
    pub part_size: u64,
    pub part_count: u64,
}

fn string(item: &HashMap<String, AttributeValue>, name: &str) -> Result<String, StoreError> {
    item.get(name)
        .and_then(|value| value.as_s().ok())
        .cloned()
        .ok_or_else(|| StoreError::Dynamo(format!("brak atrybutu {name}")))
}

fn number(item: &HashMap<String, AttributeValue>, name: &str) -> Result<u64, StoreError> {
    item.get(name)
        .and_then(|value| value.as_n().ok())
        .and_then(|n| n.parse().ok())
        .ok_or_else(|| StoreError::Dynamo(format!("brak atrybutu {name}")))
}

fn parse_status(value: &str) -> Result<AssetStatus, StoreError> {
    AssetStatus::ALL
        .into_iter()
        .find(|status| status.as_str() == value)
        .ok_or_else(|| StoreError::Dynamo(format!("nieznany status {value}")))
}

impl UploadSession {
    /// # Errors
    ///
    /// [`StoreError::Dynamo`], gdy w rekordzie brakuje pól uploadu.
    pub fn from_item(item: &HashMap<String, AttributeValue>) -> Result<Self, StoreError> {
        Ok(Self {
            asset_id: string(item, "assetId")?,
            status: parse_status(&string(item, "status")?)?,
            uploader_id: string(item, "uploaderId")?,
            upload_id: string(item, "uploadId")?,
            declared_size: number(item, "declaredSize")?,
            part_size: number(item, "partSize")?,
            part_count: number(item, "partCount")?,
        })
    }
}

/// Odczyt stanu uploadu (silnie spójny).
///
/// # Errors
///
/// [`StoreError::NotFound`] dla nieistniejącego assetu.
pub async fn get_upload_session(
    client: &Client,
    table: &str,
    asset_id: &str,
) -> Result<UploadSession, StoreError> {
    let output = client
        .get_item()
        .table_name(table)
        .key("pk", AttributeValue::S(asset_pk(asset_id)))
        .consistent_read(true)
        .send()
        .await
        .map_err(|error| StoreError::Dynamo(format!("{error:?}")))?;
    let item = output.item.ok_or(StoreError::NotFound)?;
    UploadSession::from_item(&item)
}

/// Status i autor assetu.
///
/// # Errors
///
/// [`StoreError::NotFound`] dla nieistniejącego assetu.
pub async fn get_status(
    client: &Client,
    table: &str,
    asset_id: &str,
) -> Result<(AssetStatus, String), StoreError> {
    let output = client
        .get_item()
        .table_name(table)
        .key("pk", AttributeValue::S(asset_pk(asset_id)))
        .projection_expression("#status, uploaderId")
        .expression_attribute_names("#status", "status")
        .consistent_read(true)
        .send()
        .await
        .map_err(|error| StoreError::Dynamo(format!("{error:?}")))?;
    let item = output.item.ok_or(StoreError::NotFound)?;
    Ok((
        parse_status(&string(&item, "status")?)?,
        string(&item, "uploaderId")?,
    ))
}

/// Warunkowa zmiana statusu. `extra` to dodatkowe atrybuty ustawiane razem
/// ze statusem (np. powód odrzucenia).
///
/// # Errors
///
/// [`StoreError::InvalidTransition`], gdy obecny status nie pozwala na
/// przejście (albo asset nie istnieje).
pub async fn transition(
    client: &Client,
    table: &str,
    asset_id: &str,
    to: AssetStatus,
    extra: &[(&str, AttributeValue)],
) -> Result<(), StoreError> {
    let predecessors = to.allowed_predecessors();
    if predecessors.is_empty() {
        return Err(StoreError::InvalidTransition(to));
    }
    let placeholders: Vec<String> = (0..predecessors.len()).map(|i| format!(":from{i}")).collect();
    let mut update = "SET #status = :to, updatedAt = :now".to_owned();
    let mut request = client
        .update_item()
        .table_name(table)
        .key("pk", AttributeValue::S(asset_pk(asset_id)))
        .condition_expression(format!("#status IN ({})", placeholders.join(", ")))
        .expression_attribute_names("#status", "status")
        .expression_attribute_values(":to", AttributeValue::S(to.as_str().to_owned()))
        .expression_attribute_values(":now", AttributeValue::N(now_millis().to_string()));
    for (placeholder, status) in placeholders.iter().zip(predecessors) {
        request =
            request.expression_attribute_values(placeholder, AttributeValue::S(status.as_str().to_owned()));
    }
    for (i, (name, value)) in extra.iter().enumerate() {
        let _ = write!(update, ", #x{i} = :x{i}");
        request = request
            .expression_attribute_names(format!("#x{i}"), *name)
            .expression_attribute_values(format!(":x{i}"), value.clone());
    }
    request.update_expression(update).send().await.map_err(|error| {
        let conditional = error.as_service_error().is_some_and(
            aws_sdk_dynamodb::operation::update_item::UpdateItemError::is_conditional_check_failed_exception,
        );
        if conditional {
            StoreError::InvalidTransition(to)
        } else {
            StoreError::Dynamo(format!("{error:?}"))
        }
    })?;
    Ok(())
}

/// Jak [`transition`], ale powtórzenie przejścia już wykonanego (asset ma
/// docelowy status) nie jest błędem. Kroki Step Functions mogą być
/// ponawiane, więc muszą być idempotentne (rozdział 7.1).
///
/// # Errors
///
/// [`StoreError::InvalidTransition`], gdy asset ma inny status niż
/// dozwolony poprzednik albo docelowy.
pub async fn transition_idempotent(
    client: &Client,
    table: &str,
    asset_id: &str,
    to: AssetStatus,
    extra: &[(&str, AttributeValue)],
) -> Result<(), StoreError> {
    match transition(client, table, asset_id, to, extra).await {
        Err(StoreError::InvalidTransition(_)) => match get_status(client, table, asset_id).await? {
            (current, _) if current == to => Ok(()),
            _ => Err(StoreError::InvalidTransition(to)),
        },
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item() -> HashMap<String, AttributeValue> {
        HashMap::from([
            ("assetId".to_owned(), AttributeValue::S("a1".to_owned())),
            ("status".to_owned(), AttributeValue::S("UPLOADING".to_owned())),
            ("uploaderId".to_owned(), AttributeValue::S("u1".to_owned())),
            ("uploadId".to_owned(), AttributeValue::S("mpu".to_owned())),
            ("declaredSize".to_owned(), AttributeValue::N("25".to_owned())),
            ("partSize".to_owned(), AttributeValue::N("10".to_owned())),
            ("partCount".to_owned(), AttributeValue::N("3".to_owned())),
        ])
    }

    #[test]
    fn reads_upload_session() {
        let session = UploadSession::from_item(&item()).unwrap();
        assert_eq!(session.status, AssetStatus::Uploading);
        assert_eq!(session.part_count, 3);
    }

    #[test]
    fn rejects_incomplete_records() {
        let mut broken = item();
        broken.remove("uploadId");
        assert!(UploadSession::from_item(&broken).is_err());
    }

    #[test]
    fn asset_key_has_prefix() {
        assert_eq!(asset_pk("abc"), "ASSET#abc");
    }
}
