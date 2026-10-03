# Moduł `auth` — Cognito

Pula użytkowników Cognito z grupami A–D, domeną managed login i klientem publicznym dla SPA (OAuth 2.0 authorization code + PKCE). Opcjonalnie klient do testów e2e.

## Co tworzy

| Zasób | Konfiguracja |
|---|---|
| `aws_cognito_user_pool.this` | tier **Essentials**; login = e-mail (bez rozróżniania wielkości liter); **rejestracja wyłączona** (`allow_admin_create_user_only`), zaproszenie e-mail z hasłem tymczasowym (7 dni); hasło ≥ 12 znaków, małe + wielkie litery + cyfry; **MFA opcjonalne (TOTP)**; odzyskiwanie przez zweryfikowany e-mail; wysyłka maili Cognito (limit ~50/dzień) |
| `aws_cognito_user_group.this` | `admin` (A, precedence 1), `staff` (B, 2), `contributor` (C, 3), `viewer` (D, 4) — definiowane w `envs/dev/main.tf` |
| `aws_cognito_user_pool_domain.this` | `<prefiks>.auth.eu-central-1.amazoncognito.com`, managed login v2 |
| `aws_cognito_user_pool_client.spa` | klient publiczny bez sekretu; tylko flow `code`; scope `openid email profile`; `ALLOW_USER_SRP_AUTH` + refresh (bez `USER_PASSWORD_AUTH`); access/id token 60 min, refresh 12 h; `prevent_user_existence_errors`; unieważnianie tokenów |
| `aws_cognito_managed_login_branding.spa` | domyślny wygląd strony logowania |
| `aws_cognito_user_pool_client.e2e` (gdy `create_e2e_client`) | `ALLOW_ADMIN_USER_PASSWORD_AUTH` (wymaga poświadczeń AWS z `cognito-idp:AdminInitiateAuth` — przeglądarka nie może go użyć); tokeny 15 min |

## Zmienne

| Zmienna | Opis |
|---|---|
| `name` | nazwa puli i prefiks klientów |
| `domain_prefix` | prefiks domeny managed login (globalnie unikalny, dlatego z ID konta) |
| `callback_urls`, `logout_urls` | adresy SPA (CloudFront + `localhost:4200`) |
| `groups` | mapa `nazwa => { description, precedence }` |
| `deletion_protection` | domyślnie `false` (dev) |
| `create_e2e_client` | domyślnie `false`; w dev `true` |

Outputs: `user_pool_id`, `client_id`, `e2e_client_id`, `issuer_url`, `domain_url` i inne ([`outputs.tf`](outputs.tf)).

## Jak to się łączy z resztą

- `issuer_url` i `client_id` (oraz `e2e_client_id`) trafiają do autoryzatora JWT w `http-api` — token z innej puli lub innego klienta nie przejdzie.
- Access token Cognito nie ma `aud`; autoryzator HTTP API sprawdza wtedy `client_id`.
- Claim `cognito:groups` w tokenie to podstawa autoryzacji w Lambdach (`shared::auth`).
- Konta: `scripts/create-user.sh <email> <grupa>`.
