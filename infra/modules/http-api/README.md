# Moduł `http-api` — API Gateway

HTTP API (API Gateway v2) z autoryzatorem JWT Cognito. **Każda trasa wymaga poprawnego tokenu**; sprawdzanie grup robi już Lambda (rozdział 4 projektu). Trasy i funkcje przekazuje środowisko (`envs/dev/main.tf`).

## Co tworzy

| Zasób | Konfiguracja |
|---|---|
| `aws_apigatewayv2_api.this` | `protocol_type = HTTP`; CORS: originy SPA, metody `GET POST PUT PATCH DELETE OPTIONS`, nagłówki `authorization`, `content-type`, cache preflight 1 h |
| `aws_apigatewayv2_authorizer.cognito` | typ JWT, token z nagłówka `Authorization`, `issuer` = pula Cognito, `audience` = ID klientów (SPA, e2e) |
| `aws_apigatewayv2_stage.default` | `$default`, auto-deploy, **throttling 10 req/s, burst 20** na trasę, logi dostępu JSON |
| `aws_cloudwatch_log_group.access` | `/aws/apigateway/<name>`, 14 dni |
| `aws_apigatewayv2_integration.lambda` (na trasę) | `AWS_PROXY`, payload 2.0, timeout 10 s |
| `aws_apigatewayv2_route.this` (na trasę) | `authorization_type = JWT` |
| `aws_lambda_permission.api` (na trasę) | wywołanie funkcji tylko z **tej trasy tego API** (`source_arn` z metodą i ścieżką, `{param}` → `*`) |

## Autoryzator JWT

API Gateway sam weryfikuje podpis tokenu (klucze JWKS puli), `iss`, czas ważności i `client_id` (access token Cognito nie ma `aud`). Żądanie bez tokenu, z tokenem wygasłym lub z innej puli kończy się **401 bez wywołania Lambdy** (scenariusz e2e 16). Claimy (`sub`, `email`, `cognito:groups`, …) trafiają do Lambdy w `requestContext.authorizer.jwt.claims`.

## Logi dostępu

```json
{
  "requestId": "…", "requestTime": "…", "ip": "203.0.113.7",
  "routeKey": "POST /uploads", "status": "201", "latencyMs": "84",
  "sub": "3c4f…", "authorizerErr": "-", "integrationErr": "-"
}
```

Bez nagłówków i treści żądań — tylko metadane potrzebne do analizy incydentu.

## Zmienne

| Zmienna | Domyślnie | Opis |
|---|---|---|
| `name` | — | nazwa API |
| `allowed_origins` | — | originy CORS |
| `jwt_issuer` | — | URL issuera Cognito |
| `jwt_audience` | — | lista `client_id` |
| `routes` | — | `"METODA /ścieżka" => { function_name, function_arn }` |
| `throttling_burst_limit` / `throttling_rate_limit` | 20 / 10 | |
| `log_retention_days` | 14 | |

Outputs: `api_id`, `url`.

## Trasy w `dev`

`GET /me`, `POST /uploads`, `GET /uploads/{assetId}`, `POST /uploads/{assetId}/complete`, `GET /assets`, `GET /assets/{assetId}/download`, `POST /assets/{assetId}/publish`, `POST /assets/{assetId}/rescan`, `DELETE /assets/{assetId}` — opis: [`docs/api.md`](../../../docs/api.md).
