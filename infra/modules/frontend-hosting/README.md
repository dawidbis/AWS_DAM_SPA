# Moduł `frontend-hosting` — S3 + CloudFront

Hosting Angular SPA: prywatny bucket S3 dostępny wyłącznie przez CloudFront (Origin Access Control), nagłówki bezpieczeństwa i plik `config.json` z konfiguracją środowiska.

## Co tworzy

| Zasób | Konfiguracja |
|---|---|
| `aws_s3_bucket.site` | prywatny (Block Public Access, `BucketOwnerEnforced`), SSE-S3, wersjonowanie, logi do `access-logs`, lifecycle starych wersji |
| `aws_cloudfront_origin_access_control.site` | podpisywanie żądań do S3 (SigV4) |
| `aws_s3_bucket_policy.site` | `s3:GetObject` tylko dla tej dystrybucji (`AWS:SourceArn`) |
| `aws_cloudfront_response_headers_policy.security` | CSP, HSTS, `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`, `Referrer-Policy`, `Permissions-Policy` |
| `aws_cloudfront_distribution.site` | HTTPS (redirect z HTTP), `index.html` jako root, `PriceClass_100`, logi standardowe do `access-logs`; **403/404 → `/index.html` (200)** dla routingu SPA |
| `aws_s3_object.config` | `config.json` z `runtime_config` |

## Content Security Policy

```
default-src 'self';
script-src 'self';
style-src 'self' 'unsafe-inline';
img-src 'self' data: https://*.s3.eu-central-1.amazonaws.com;
connect-src 'self' https://*.execute-api.eu-central-1.amazonaws.com
            https://*.auth.eu-central-1.amazoncognito.com
            https://cognito-idp.eu-central-1.amazonaws.com
            https://*.s3.eu-central-1.amazonaws.com;
worker-src 'self'; font-src 'self'; object-src 'none';
base-uri 'self'; form-action 'self'; frame-ancestors 'none';
upgrade-insecure-requests
```

- **Skrypty tylko z własnej domeny**: Angular bez `eval` i bez skryptów inline (`inlineCritical: false` w `angular.json`).
- `style-src 'unsafe-inline'`: Angular wstrzykuje style komponentów jako `<style>`; nonce wymagałby renderowania po stronie serwera (ryzyko zaakceptowane).
- `img-src`/`connect-src` z S3: miniatury i podglądy (presigned GET) oraz wysyłanie części pliku (presigned PUT z Web Workera).
- Hosty jako **wzorce regionu**, bo dokładne adresy API i Cognito zależą od adresu CloudFront (cykl w Terraform).

## `config.json`

Jeden build frontendu działa w każdym środowisku: Angular czyta przy starcie `/config.json`:

```json
{
  "region": "eu-central-1",
  "authority": "https://cognito-idp.eu-central-1.amazonaws.com/<pool-id>",
  "authDomain": "https://<prefiks>.auth.eu-central-1.amazoncognito.com",
  "clientId": "<client-id>",
  "apiUrl": "https://<api-id>.execute-api.eu-central-1.amazonaws.com"
}
```

Lokalnie ten sam plik tworzy `just frontend-config`.

## Deploy plików

Terraform tworzy bucket i dystrybucję, a pliki Angulara wgrywa `scripts/deploy-frontend.sh` (krok w `deploy.yml`): `aws s3 sync` z odpowiednim `Cache-Control` (zasoby z hashem w nazwie — długo, `index.html` — bez cache) i invalidacja CloudFront.

## Zmienne / outputs

Zmienne: `name`, `bucket_name`, `runtime_config`, `log_bucket_id`, `log_bucket_domain_name`. Outputs: `url`, `bucket_name`, `distribution_id`.
