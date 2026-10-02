# Matchday DAM

**Digital Asset Management z wbudowanym antywirusem dla fikcyjnego klubu piłkarskiego KS Matchday.**

Serverless na AWS: Angular SPA → API Gateway → Lambdy w Ruście → S3 (kwarantanna) → EventBridge → SQS → Step Functions → ClamAV / CDR → galeria. Każdy plik od użytkownika jest niezaufany, dopóki pipeline bezpieczeństwa nie potwierdzi, że jest czysty.

Pełny opis projektu, architektura i plan etapów: [`docs/PROJEKT.md`](docs/PROJEKT.md).

## Status

| Etap | Zakres | Stan |
|---|---|---|
| 0 — Fundamenty | Monorepo, bootstrap AWS (stan, OIDC, role CI, budżety), Lambda „hello world" w Ruście, CI/CD | ✅ kod gotowy, wymaga jednorazowego [bootstrapu konta](docs/setup-aws.md) |
| 1 — Rdzeń | Cognito, upload multipart, ClamAV, galeria | 🚧 w toku: Cognito, hosting SPA, logowanie |
| 2 — Bezpieczeństwo w głąb | Step Functions, walidacja, CDR, renditions | — |
| 3 — Domena | Słowniki, prawa wizerunkowe, alarmy | — |
| 4 — Portfolio | ADR-y, demo, nagranie | — |

## Struktura

```
docs/            PROJEKT.md, setup-aws.md, ADR-y
frontend/        Angular 22 (standalone, signals, zoneless) + Tailwind CSS 4 + daisyUI 5
lambdas/         cargo workspace: shared (modele, statusy), hello-world
infra/bootstrap/ jednorazowo: bucket stanu, OIDC GitHub, role dam-github-*, permission boundary, budżety
infra/modules/   moduły Terraform: rust-lambda, auth (Cognito), frontend-hosting (S3 + CloudFront)
scripts/         deploy frontendu, zakładanie kont testowych
infra/envs/dev/  środowisko dev (backend S3 z use_lockfile)
.github/         CI (lint/test/scan), plan w PR, deploy po merge
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
| `deploy.yml` | push do `main` | CI → `terraform apply` jako rola `dam-github-deploy` → smoke test Lambdy |

GitHub Actions łączy się z AWS wyłącznie przez OIDC. W repozytorium nie ma kluczy dostępowych.

## Decyzje architektoniczne

[`docs/adr/`](docs/adr/)

## Licencja

MIT. Klub, herby i dane są fikcyjne.
