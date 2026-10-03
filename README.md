# Matchday DAM

**Digital Asset Management z wbudowanym antywirusem dla fikcyjnego klubu piłkarskiego KS Matchday.**

Serverless na AWS: Angular SPA → API Gateway → Lambdy w Ruście → S3 (kwarantanna) → EventBridge → SQS → Step Functions → ClamAV / CDR → galeria. Każdy plik od użytkownika jest niezaufany, dopóki pipeline bezpieczeństwa nie potwierdzi, że jest czysty.

Jak to działa w AWS (diagramy, przepływy, uprawnienia): [`docs/architecture.md`](docs/architecture.md). Pełny opis projektu i plan etapów: [`docs/PROJEKT.md`](docs/PROJEKT.md).

## Status

| Etap | Zakres | Stan |
|---|---|---|
| 0 — Fundamenty | Monorepo, bootstrap AWS (stan, OIDC, role CI, budżety), Lambda „hello world" w Ruście, CI/CD | ✅ |
| 1 — Rdzeń | Cognito, hosting SPA, API z JWT, upload multipart z wznawianiem, skan ClamAV, galeria, publikacja | ✅ |
| 2 — Bezpieczeństwo w głąb | Step Functions, walidacja magic bytes, CDR, miniatury i podglądy ze znakiem wodnym, JSON Schema, CSP, testy e2e, model zagrożeń, usuwanie assetów | ✅ |
| 3 — Domena | Słowniki, prawa wizerunkowe, embarga, panel incydentów, alarmy CloudWatch | — |
| 4 — Portfolio | ADR-y, demo, nagranie | — |

## Dokumentacja

| Dokument | Co opisuje |
|---|---|
| [`docs/architecture.md`](docs/architecture.md) | **Architektura chmurowa**: komponenty AWS, diagramy przepływów (logowanie, upload, pipeline, publikacja, incydent), statusy, buckety, tabele, role IAM, limity, CI/CD |
| [`docs/api.md`](docs/api.md) | Kontrakt HTTP API: trasy, żądania, odpowiedzi, kody błędów |
| [`lambdas/README.md`](lambdas/README.md) | Mapa wszystkich Lambd i wspólne zasady kodu; **każda funkcja ma własny README** w swoim katalogu |
| [`lambdas/shared/README.md`](lambdas/shared/README.md) | Wspólna biblioteka: statusy, autoryzacja, kontrakty pipeline'u |
| [`infra/README.md`](infra/README.md) | Terraform: struktura, zależności modułów, konwencje; **każdy moduł ma własny README** |
| [`frontend/README.md`](frontend/README.md) | Angular SPA: struktura, trasy, upload z Web Workerem |
| [`tests/README.md`](tests/README.md) | Testy jednostkowe, pliki ataków, scenariusze e2e |
| [`docs/threat-model.md`](docs/threat-model.md) | Model zagrożeń STRIDE |
| [`docs/setup-aws.md`](docs/setup-aws.md) | Pierwsze wdrożenie na AWS krok po kroku |
| [`docs/adr/`](docs/adr/) | Decyzje architektoniczne |
| [`docs/PROJEKT.md`](docs/PROJEKT.md) | Pełny opis projektu i plan etapów |

## Struktura

```
docs/            architektura, API, model zagrożeń, setup AWS, ADR-y, opis projektu
frontend/        Angular 22 (standalone, signals, zoneless) + Tailwind CSS 4 + daisyUI 5
lambdas/         cargo workspace (Rust, edition 2024):
  shared/          modele, statusy, autoryzacja, upload, katalog, kontrakty pipeline'u
  api/             me, upload-init, upload-status, upload-complete, assets-read,
                   asset-publish, asset-rescan, asset-delete
  pipeline/        start-scan, scan (ClamAV, obraz kontenera), validate, cdr,
                   renditions, finalize-clean, handle-infected
  hello-world/     smoke test łańcucha build → deploy
infra/
  bootstrap/       jednorazowo: bucket stanu, OIDC GitHub, role dam-github-*, permission boundary, budżety
  modules/         rust-lambda, auth (Cognito), frontend-hosting (S3 + CloudFront), access-logs,
                   storage (quarantine/clean/renditions/infected), http-api (API Gateway + JWT),
                   data (DynamoDB assets, incidents), scanner (SQS, Step Functions, Lambdy pipeline'u, SNS)
  envs/dev/        środowisko dev (backend S3 z use_lockfile)
scripts/         build obrazu skanera, deploy frontendu, zakładanie kont
tests/           e2e scenariuszy bezpieczeństwa, pliki ataków
.github/         CI, plan w PR, deploy po merge, e2e
justfile         build, check, bootstrap, plan, deploy, destroy
```

## Wymagania lokalne

| Narzędzie | Wersja |
|---|---|
| Rust | stable (edition 2024) + target `aarch64-unknown-linux-gnu` |
| [Cargo Lambda](https://www.cargo-lambda.info/) | `pip install cargo-lambda` lub `brew install cargo-lambda/tap/cargo-lambda` |
| Node.js | 24 LTS (`frontend/.nvmrc`) |
| Terraform | ≥ 1.10 (CI używa 1.16) |
| AWS CLI | v2 |
| [just](https://github.com/casey/just) | dowolna aktualna |
| tflint, checkov | opcjonalnie (uruchamia je CI) |

## Quick start

```bash
just check            # to samo co CI: fmt, clippy, testy, ng lint/test, terraform validate, tflint, checkov
just build-lambdas    # lambdas/target/lambda/<crate>/bootstrap.zip (arm64)
just frontend-config                # config.json z outputów Terraform (Cognito)
cd frontend && npm ci --ignore-scripts && npm start   # http://localhost:4200
```

Pierwsze wdrożenie na AWS (bootstrap konta, zmienne GitHub, deploy): [`docs/setup-aws.md`](docs/setup-aws.md).

## CI/CD

| Workflow | Kiedy | Co robi |
|---|---|---|
| `ci.yml` | każdy PR | `cargo fmt/clippy/test`, `ng lint/test/build`, `terraform fmt/validate`, `tflint`, Checkov, build Lambd arm64 |
| `plan.yml` | PR zmieniający `infra/` lub `lambdas/` | `terraform plan` jako rola `dam-github-plan` (tylko odczyt), wynik w komentarzu PR |
| `deploy.yml` | push do `main`, co tydzień, ręcznie | CI → obraz skanera (gdy trzeba) → `terraform apply` jako rola `dam-github-deploy` → frontend → smoke testy |
| `e2e.yml` | ręcznie | scenariusze bezpieczeństwa z rozdziału 12 na środowisku dev (`tests/e2e/run.sh`) |

GitHub Actions łączy się z AWS wyłącznie przez OIDC. W repozytorium nie ma kluczy dostępowych.

## Decyzje architektoniczne

[`docs/adr/`](docs/adr/)

## Licencja

MIT. Klub, herby i dane są fikcyjne.
