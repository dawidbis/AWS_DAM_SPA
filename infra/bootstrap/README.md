# Bootstrap konta AWS

Jednorazowa konfiguracja konta, którą uruchamia administrator (np. w CloudShell) **przed** pierwszym deployem z GitHub Actions. Tworzy to, czego CI nie może utworzyć samo dla siebie: bucket stanu, role OIDC i granicę uprawnień. Instrukcja: [`docs/setup-aws.md`](../../docs/setup-aws.md).

Pierwsze `apply` bootstrapu działa na stanie lokalnym (bucket stanu jeszcze nie istnieje, `backend_override.tf`), a potem stan jest przenoszony do tego samego bucketu S3 pod osobnym kluczem (kroki 3–4 w [`docs/setup-aws.md`](../../docs/setup-aws.md)). Kolejne zmiany: `terraform -chdir=infra/bootstrap init -backend-config="bucket=<TF_STATE_BUCKET>"` i `apply` z konta administratora.

## Pliki

| Plik | Zasoby | Po co |
|---|---|---|
| [`state.tf`](state.tf) | bucket `matchday-dam-tfstate-<konto>` | stan Terraform wszystkich środowisk; wersjonowanie, szyfrowanie, Block Public Access; blokady przez plik `.tflock` (`use_lockfile`), bez tabeli DynamoDB |
| [`github_oidc.tf`](github_oidc.tf) | dostawca OIDC `token.actions.githubusercontent.com`, role `dam-github-plan` i `dam-github-deploy` | logowanie GitHub Actions do AWS bez kluczy |
| [`permissions_boundary.tf`](permissions_boundary.tf) | polityka `dam-permissions-boundary` | maksymalny zakres uprawnień każdej roli projektu |
| [`budgets.tf`](budgets.tf) | budżety miesięczne 5 i 20 USD | e-mail przy przekroczeniu |
| [`variables.tf`](variables.tf) | region, projekt, repozytorium, ID właściciela i repo GitHub, gałąź deployu, progi budżetu, e-mail | |

## Role GitHub Actions

| Rola | Kto może ją przyjąć (warunek `sub` tokenu OIDC) | Uprawnienia | Workflow |
|---|---|---|---|
| `dam-github-plan` | `repo:<owner>@<id>/<repo>@<id>:pull_request` | `ReadOnlyAccess` + zapis/odczyt pliku blokady stanu | `plan.yml` |
| `dam-github-deploy` | `…:ref:refs/heads/main` | `PowerUserAccess` + zarządzanie rolami/politykami `dam-*` (tylko z boundary), `iam:PassRole` dla `dam-*`, `iam:SimulatePrincipalPolicy` dla `dam-*` (scenariusz e2e 15) | `deploy.yml`, `e2e.yml` (uruchamiany z `main`) |

`sub` zawiera **numeryczne ID** właściciela i repozytorium, więc po usunięciu i ponownym utworzeniu repozytorium o tej samej nazwie (inne ID) role przestają działać — ochrona przed przejęciem nazwy.

## Permission boundary

Boundary nie nadaje uprawnień, tylko wyznacza ich górną granicę: efektywne uprawnienia roli to część wspólna jej polityk i boundary. Gwarancje:

1. **Tylko usługi projektu** (API Gateway, Budgets, CloudFront, CloudWatch, Cognito, DynamoDB, ECR, EventBridge, Lambda, Logs, S3, SNS, SQS, Step Functions, WAF, X-Ray, …) i **tylko region projektu** (+ `us-east-1` dla usług globalnych).
2. **Brak ucieczki**: rola może tworzyć role tylko z tym samym boundary (`DenyRolesWithoutThisBoundary`) i nie może go zdjąć (`DenyBoundaryRemoval`).
3. **Nienaruszalność**: nikt z boundary nie zmieni boundary (`DenyBoundaryPolicyChanges`) ani ról `dam-github-*` (`DenyGitHubRoleChanges`).
4. **Ochrona stanu**: bucketu stanu nie da się usunąć ani zmienić jego polityki (`ProtectTerraformState`).

Uzasadnienie i alternatywy: [ADR 0013](../../docs/adr/0013-github-oidc-deploy-roles.md).

## Pierwsze uruchomienie (skrót; pełne kroki ze stanem w S3: `docs/setup-aws.md` 3–5)

```bash
cd infra/bootstrap
cp terraform.tfvars.example terraform.tfvars   # e-mail do budżetów
terraform init && terraform apply
terraform output   # → zmienne repozytorium GitHub: AWS_REGION, AWS_PLAN_ROLE_ARN, AWS_DEPLOY_ROLE_ARN, TF_STATE_BUCKET
```

Zmiany w bootstrapie (np. nowe uprawnienie boundary) też wymagają ponownego `terraform apply` przez administratora — CI celowo nie ma do tego uprawnień.
