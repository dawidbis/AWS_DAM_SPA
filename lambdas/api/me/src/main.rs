//! `GET /me`: zwraca tożsamość i grupy wywołującego tak, jak widzi je backend.
//!
//! Pozwala sprawdzić cały łańcuch: token z SPA → autoryzator JWT API Gateway
//! → claimy w Lambdzie. Frontend pokazuje wynik na stronie głównej.

use lambda_http::request::RequestContext;
use lambda_http::{Body, Error, Request, RequestExt, Response, http::StatusCode, service_fn};
use serde_json::json;
use shared::Caller;

fn json_response(status: StatusCode, body: &serde_json::Value) -> Result<Response<Body>, Error> {
    Ok(Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .header("cache-control", "no-store")
        .body(Body::Text(body.to_string()))?)
}

fn caller(request: &Request) -> Option<Caller> {
    let RequestContext::ApiGatewayV2(context) = request.request_context() else {
        return None;
    };
    let claims = context.authorizer?.jwt?.claims;
    Caller::from_claims(&claims).ok()
}

#[allow(clippy::unused_async)]
async fn handler(request: Request) -> Result<Response<Body>, Error> {
    // Bez claimów (np. błędna konfiguracja trasy bez autoryzatora) odmawiamy.
    let Some(caller) = caller(&request) else {
        tracing::warn!("request without JWT claims");
        return json_response(StatusCode::UNAUTHORIZED, &json!({ "message": "Unauthorized" }));
    };

    tracing::info!(sub = %caller.sub, groups = ?caller.groups, "me");
    json_response(StatusCode::OK, &json!(caller))
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    shared::telemetry::init();
    lambda_http::run(service_fn(handler)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use lambda_http::aws_lambda_events::apigw::{
        ApiGatewayRequestAuthorizer, ApiGatewayRequestAuthorizerJwtDescription,
        ApiGatewayV2httpRequestContext,
    };
    use std::collections::HashMap;

    fn request_with_claims(claims: Option<HashMap<String, String>>) -> Request {
        let mut context = ApiGatewayV2httpRequestContext::default();
        if let Some(claims) = claims {
            let mut jwt = ApiGatewayRequestAuthorizerJwtDescription::default();
            jwt.claims = claims;
            let mut authorizer = ApiGatewayRequestAuthorizer::default();
            authorizer.jwt = Some(jwt);
            context.authorizer = Some(authorizer);
        }
        Request::default().with_request_context(RequestContext::ApiGatewayV2(context))
    }

    fn body_json(response: &Response<Body>) -> serde_json::Value {
        match response.body() {
            Body::Text(text) => serde_json::from_str(text).unwrap(),
            other => panic!("unexpected body {other:?}"),
        }
    }

    #[tokio::test]
    async fn returns_caller_from_claims() {
        let claims = HashMap::from([
            ("sub".to_owned(), "abc".to_owned()),
            ("email".to_owned(), "admin@example.com".to_owned()),
            ("cognito:groups".to_owned(), "[admin]".to_owned()),
        ]);

        let response = handler(request_with_claims(Some(claims))).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            body_json(&response),
            json!({ "sub": "abc", "email": "admin@example.com", "groups": ["admin"] })
        );
    }

    #[tokio::test]
    async fn rejects_request_without_claims() {
        let response = handler(request_with_claims(None)).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
