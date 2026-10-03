//! Wspólne elementy Lambd API: odpowiedzi JSON, błędy i tożsamość wywołującego.

use lambda_http::http::{HeaderValue, StatusCode};
use lambda_http::request::RequestContext;
use lambda_http::{Body, Request, RequestExt, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::json;

use crate::auth::{AuthError, Caller};

/// Błąd zwracany klientowi. Szczegóły błędów wewnętrznych trafiają tylko do logów.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    BadRequest(String),
    #[error("Unauthorized")]
    Unauthorized,
    #[error("Forbidden")]
    Forbidden,
    #[error("Not found")]
    NotFound,
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Unprocessable(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl ApiError {
    #[must_use]
    pub fn internal(error: impl std::fmt::Display) -> Self {
        Self::Internal(error.to_string())
    }

    #[must_use]
    pub const fn status(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Unprocessable(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Odpowiedź HTTP; dla błędów 500 klient dostaje tylko ogólny komunikat.
    #[must_use]
    pub fn into_response(self) -> Response<Body> {
        let message = match &self {
            Self::Internal(detail) => {
                tracing::error!(error = %detail, "internal error");
                "Internal error".to_owned()
            }
            other => other.to_string(),
        };
        json_response(self.status(), &json!({ "message": message }))
    }
}

impl From<AuthError> for ApiError {
    fn from(error: AuthError) -> Self {
        match error {
            AuthError::MissingSubject => Self::Unauthorized,
            AuthError::Forbidden => Self::Forbidden,
        }
    }
}

#[must_use]
pub fn json_response(status: StatusCode, body: &impl Serialize) -> Response<Body> {
    let body = serde_json::to_string(body).unwrap_or_else(|_| "{}".to_owned());
    let mut response = Response::new(Body::Text(body));
    *response.status_mut() = status;
    let headers = response.headers_mut();
    headers.insert("content-type", HeaderValue::from_static("application/json"));
    headers.insert("cache-control", HeaderValue::from_static("no-store"));
    response
}

/// Zamienia wynik handlera na odpowiedź HTTP.
#[must_use]
pub fn respond<T: Serialize>(result: Result<(StatusCode, T), ApiError>) -> Response<Body> {
    match result {
        Ok((status, body)) => json_response(status, &body),
        Err(error) => error.into_response(),
    }
}

/// Wywołujący na podstawie claimów autoryzatora JWT. Brak claimów = 401.
///
/// # Errors
///
/// [`ApiError::Unauthorized`], gdy żądanie nie przeszło przez autoryzator.
pub fn caller(request: &Request) -> Result<Caller, ApiError> {
    let Some(RequestContext::ApiGatewayV2(context)) = request.request_context_ref() else {
        return Err(ApiError::Unauthorized);
    };
    let claims = context
        .authorizer
        .as_ref()
        .and_then(|authorizer| authorizer.jwt.as_ref())
        .map(|jwt| &jwt.claims)
        .ok_or(ApiError::Unauthorized)?;
    Ok(Caller::from_claims(claims)?)
}

/// Parsuje ciało żądania JSON (limit rozmiaru egzekwuje API Gateway).
///
/// # Errors
///
/// [`ApiError::BadRequest`] dla pustego lub niepoprawnego JSON-a.
pub fn json_body<T: DeserializeOwned>(request: &Request) -> Result<T, ApiError> {
    let bytes: &[u8] = match request.body() {
        Body::Text(text) => text.as_bytes(),
        Body::Binary(bytes) => bytes,
        Body::Empty => return Err(ApiError::BadRequest("Missing request body".to_owned())),
        _ => return Err(ApiError::BadRequest("Unsupported request body".to_owned())),
    };
    serde_json::from_slice(bytes).map_err(|error| ApiError::BadRequest(format!("Invalid JSON: {error}")))
}

/// Parametr ścieżki (np. `{assetId}`).
///
/// # Errors
///
/// [`ApiError::BadRequest`], gdy parametru brak.
pub fn path_param(request: &Request, name: &str) -> Result<String, ApiError> {
    request
        .path_parameters_ref()
        .and_then(|params| params.first(name))
        .map(str::to_owned)
        .ok_or_else(|| ApiError::BadRequest(format!("Missing path parameter {name}")))
}

/// Opcjonalny parametr query stringa (np. `?view=gallery`).
#[must_use]
pub fn query_param(request: &Request, name: &str) -> Option<String> {
    request
        .query_string_parameters_ref()
        .and_then(|params| params.first(name))
        .map(str::to_owned)
}

/// Konfiguracja z zmiennej środowiskowej Lambdy.
///
/// # Panics
///
/// Gdy zmienna nie istnieje: to błąd wdrożenia, funkcja nie powinna startować.
#[must_use]
pub fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("missing environment variable {name}"))
}

#[cfg(any(test, feature = "testing"))]
pub mod testing {
    //! Budowanie żądań API Gateway w testach handlerów.

    use std::collections::HashMap;

    use lambda_http::aws_lambda_events::apigw::{
        ApiGatewayRequestAuthorizer, ApiGatewayRequestAuthorizerJwtDescription,
        ApiGatewayV2httpRequestContext,
    };
    use lambda_http::request::RequestContext;
    use lambda_http::{Request, RequestExt};

    #[must_use]
    pub fn request_as(sub: &str, groups: &str) -> Request {
        let mut jwt = ApiGatewayRequestAuthorizerJwtDescription::default();
        jwt.claims = HashMap::from([
            ("sub".to_owned(), sub.to_owned()),
            ("cognito:groups".to_owned(), groups.to_owned()),
        ]);
        let mut authorizer = ApiGatewayRequestAuthorizer::default();
        authorizer.jwt = Some(jwt);
        let mut context = ApiGatewayV2httpRequestContext::default();
        context.authorizer = Some(authorizer);
        Request::default().with_request_context(RequestContext::ApiGatewayV2(context))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UserGroup;

    #[test]
    fn reads_caller_from_authorizer_claims() {
        let caller = caller(&testing::request_as("abc", "[contributor]")).unwrap();
        assert_eq!(caller.sub, "abc");
        assert_eq!(caller.groups, vec![UserGroup::Contributor]);
    }

    #[test]
    fn request_without_authorizer_is_unauthorized() {
        assert!(matches!(caller(&Request::default()), Err(ApiError::Unauthorized)));
    }

    #[test]
    fn internal_errors_hide_details() {
        let response = ApiError::internal("dynamodb exploded").into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let Body::Text(body) = response.body() else {
            panic!("expected text body")
        };
        assert!(!body.contains("dynamodb"));
    }

    #[test]
    fn rejects_invalid_json() {
        let request = Request::new(Body::Text("{".to_owned()));
        assert!(matches!(
            json_body::<serde_json::Value>(&request),
            Err(ApiError::BadRequest(_))
        ));
    }
}
