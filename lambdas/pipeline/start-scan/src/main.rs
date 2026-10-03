//! Lambda `start-scan`: komunikat z kolejki skanowania (zdarzenie S3 przez
//! EventBridge) uruchamia wykonanie Step Functions `scan-pipeline`.
//!
//! Nazwa wykonania to ID assetu, więc powtórzone zdarzenie trafia na
//! `ExecutionAlreadyExists` zamiast uruchamiać pipeline drugi raz
//! (scenariusz 13). Kolejność kroków i obsługę błędów zna maszyna stanów;
//! ta funkcja tylko ją startuje (rozdział 7.1).

mod event;

use std::sync::Arc;

use aws_lambda_events::sqs::{BatchItemFailure, SqsBatchResponse, SqsEvent};
use aws_sdk_sfn::operation::start_execution::StartExecutionError;
use lambda_runtime::{Error, LambdaEvent, service_fn};
use shared::http::env;
use shared::pipeline::{StepInput, execution_name};

use crate::event::ObjectRef;

struct App {
    sfn: aws_sdk_sfn::Client,
    state_machine_arn: String,
    quarantine: String,
}

/// Co zrobić z komunikatem po próbie startu.
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    Started,
    Duplicate,
    Ignored,
}

async fn start(app: &App, object: &ObjectRef) -> Result<Outcome, String> {
    if object.bucket != app.quarantine {
        tracing::warn!(bucket = %object.bucket, "event from unexpected bucket, ignoring");
        return Ok(Outcome::Ignored);
    }
    let input = serde_json::to_string(&StepInput {
        asset_id: object.asset_id.clone(),
        scan: None,
    })
    .map_err(|e| e.to_string())?;
    let result = app
        .sfn
        .start_execution()
        .state_machine_arn(&app.state_machine_arn)
        .name(execution_name(&object.asset_id, None))
        .input(input)
        .send()
        .await;
    match result {
        Ok(_) => Ok(Outcome::Started),
        Err(error)
            if error
                .as_service_error()
                .is_some_and(StartExecutionError::is_execution_already_exists) =>
        {
            Ok(Outcome::Duplicate)
        }
        Err(error) => Err(format!("{error:?}")),
    }
}

async fn handler(app: &App, event: LambdaEvent<SqsEvent>) -> Result<SqsBatchResponse, Error> {
    let mut response = SqsBatchResponse::default();
    for record in event.payload.records {
        let id = record.message_id.clone().unwrap_or_default();
        let object = match event::parse(record.body.as_deref().unwrap_or_default()) {
            Ok(object) => object,
            Err(error) => {
                // Komunikat, którego nie rozumiemy, nie wróci do kolejki w nieskończoność.
                tracing::warn!(message_id = %id, %error, "ignoring message");
                continue;
            }
        };
        match start(app, &object).await {
            Ok(outcome) => tracing::info!(asset_id = %object.asset_id, ?outcome, "scan pipeline"),
            Err(error) => {
                tracing::error!(message_id = %id, asset_id = %object.asset_id, %error, "will retry");
                let mut failure = BatchItemFailure::default();
                failure.item_identifier = id;
                response.batch_item_failures.push(failure);
            }
        }
    }
    Ok(response)
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    shared::telemetry::init();
    let config = aws_config::load_from_env().await;
    let app = Arc::new(App {
        sfn: aws_sdk_sfn::Client::new(&config),
        state_machine_arn: env("STATE_MACHINE_ARN"),
        quarantine: env("QUARANTINE_BUCKET"),
    });
    lambda_runtime::run(service_fn(move |event: LambdaEvent<SqsEvent>| {
        let app = Arc::clone(&app);
        async move { handler(&app, event).await }
    }))
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let config = aws_sdk_sfn::Config::builder()
            .behavior_version(aws_sdk_sfn::config::BehaviorVersion::latest())
            .region(aws_sdk_sfn::config::Region::new("eu-central-1"))
            .credentials_provider(aws_sdk_sfn::config::Credentials::for_tests())
            .build();
        App {
            sfn: aws_sdk_sfn::Client::from_conf(config),
            state_machine_arn: "arn:aws:states:eu-central-1:123456789012:stateMachine:scan".to_owned(),
            quarantine: "quarantine".to_owned(),
        }
    }

    #[tokio::test]
    async fn ignores_objects_outside_quarantine() {
        let object = ObjectRef {
            bucket: "clean".to_owned(),
            asset_id: "0b6f3c1e-8a2d-4f5b-9c7e-1d2a3b4c5d6e".to_owned(),
            size: None,
        };
        assert_eq!(start(&app(), &object).await, Ok(Outcome::Ignored));
    }
}
