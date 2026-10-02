//! Zdarzenie „Object Created” z S3, przekazane przez EventBridge do SQS.
//! Komunikat zawiera tylko referencję do obiektu, nigdy treść pliku.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct EventBridgeEvent {
    #[serde(rename = "detail-type")]
    detail_type: String,
    detail: Detail,
}

#[derive(Debug, Deserialize)]
struct Detail {
    bucket: Bucket,
    object: Object,
}

#[derive(Debug, Deserialize)]
struct Bucket {
    name: String,
}

#[derive(Debug, Deserialize)]
struct Object {
    key: String,
    #[serde(default)]
    size: Option<u64>,
}

/// Obiekt w kwarantannie do przeskanowania.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectRef {
    pub bucket: String,
    pub asset_id: String,
    pub size: Option<u64>,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum EventError {
    #[error("niepoprawne zdarzenie: {0}")]
    Malformed(String),
    #[error("nieobsługiwany typ zdarzenia: {0}")]
    UnexpectedType(String),
    #[error("klucz obiektu nie jest identyfikatorem assetu")]
    UnexpectedKey,
}

/// Klucz w kwarantannie to UUID nadany przez upload-init. Każdy inny klucz
/// oznacza obiekt, który nie powstał przez nasze API, więc go ignorujemy.
fn is_asset_id(key: &str) -> bool {
    key.len() == 36
        && key.chars().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit() && !c.is_ascii_uppercase(),
        })
}

/// Parsuje ciało komunikatu SQS.
///
/// # Errors
///
/// [`EventError`] dla niepoprawnego JSON-a, innego typu zdarzenia albo klucza,
/// który nie jest UUID.
pub fn parse(body: &str) -> Result<ObjectRef, EventError> {
    let event: EventBridgeEvent =
        serde_json::from_str(body).map_err(|error| EventError::Malformed(error.to_string()))?;
    if event.detail_type != "Object Created" {
        return Err(EventError::UnexpectedType(event.detail_type));
    }
    if !is_asset_id(&event.detail.object.key) {
        return Err(EventError::UnexpectedKey);
    }
    Ok(ObjectRef {
        bucket: event.detail.bucket.name,
        asset_id: event.detail.object.key,
        size: event.detail.object.size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(detail_type: &str, key: &str) -> String {
        serde_json::json!({
            "version": "0",
            "source": "aws.s3",
            "detail-type": detail_type,
            "detail": {
                "bucket": { "name": "quarantine" },
                "object": { "key": key, "size": 1234, "etag": "x" },
                "reason": "CompleteMultipartUpload"
            }
        })
        .to_string()
    }

    const ID: &str = "0b6f3c1e-8a2d-4f5b-9c7e-1d2a3b4c5d6e";

    #[test]
    fn parses_object_created() {
        assert_eq!(
            parse(&event("Object Created", ID)).unwrap(),
            ObjectRef {
                bucket: "quarantine".to_owned(),
                asset_id: ID.to_owned(),
                size: Some(1234)
            }
        );
    }

    #[test]
    fn rejects_other_events_and_foreign_keys() {
        assert!(matches!(
            parse(&event("Object Deleted", ID)),
            Err(EventError::UnexpectedType(_))
        ));
        assert_eq!(
            parse(&event("Object Created", "../etc/passwd")),
            Err(EventError::UnexpectedKey)
        );
        assert_eq!(
            parse(&event("Object Created", &ID.to_uppercase())),
            Err(EventError::UnexpectedKey)
        );
        assert!(matches!(parse("{"), Err(EventError::Malformed(_))));
    }
}
