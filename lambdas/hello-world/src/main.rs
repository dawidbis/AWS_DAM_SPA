//! Funkcja „hello world" z etapu 0.
//!
//! Nie robi nic biznesowego. Służy do sprawdzenia, że cały łańcuch działa:
//! build Cargo Lambda (`arm64`), deploy przez Terraform z GitHub Actions
//! i logowanie JSON w CloudWatch.

use lambda_runtime::{Error, LambdaEvent, service_fn};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Response {
    message: String,
    version: &'static str,
}

#[allow(clippy::unused_async)]
async fn handler(event: LambdaEvent<Value>) -> Result<Response, Error> {
    let name = event
        .payload
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty() && name.len() <= 64)
        .unwrap_or("KS Matchday");

    tracing::info!(request_id = %event.context.request_id, "hello-world invoked");

    Ok(Response {
        message: format!("Hello, {name}!"),
        version: env!("CARGO_PKG_VERSION"),
    })
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    shared::telemetry::init();
    lambda_runtime::run(service_fn(handler)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use lambda_runtime::Context;
    use serde_json::json;

    async fn call(payload: Value) -> Response {
        handler(LambdaEvent::new(payload, Context::default()))
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn greets_by_name() {
        assert_eq!(call(json!({ "name": "Kibic" })).await.message, "Hello, Kibic!");
    }

    #[tokio::test]
    async fn falls_back_to_club_name() {
        assert_eq!(call(json!({})).await.message, "Hello, KS Matchday!");
        assert_eq!(
            call(json!({ "name": "x".repeat(65) })).await.message,
            "Hello, KS Matchday!"
        );
    }
}
