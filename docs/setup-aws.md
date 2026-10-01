# Przygotowanie konta AWS (etap 0)

Jednorazowa procedura. Po jej wykonaniu każdy merge do `main` wdraża środowisko `dev` przez GitHub Actions bez kluczy dostępowych w repozytorium.

Region projektu: **eu-central-1** (Frankfurt).

## 1. Konto root

1. Zaloguj się jako root, włącz **MFA** (Security credentials → Multi-factor authentication). Najlepiej klucz sprzętowy lub passkey.
2. Upewnij się, że root **nie ma** access keys.
3. Od tej chwili konta root nie używasz do codziennej pracy.

## 2. Użytkownik administracyjny

IAM Identity Center z dostępem do kont wymaga AWS Organizations, a dołączenie do organizacji kończy kredyty Free Planu (patrz [ADR 0012](adr/0012-single-aws-account.md)). Dlatego na tym etapie używamy **użytkownika IAM z MFA**.

1. IAM → Users → Create user, np. `admin`, z dostępem do konsoli.
2. Przypnij politykę `AdministratorAccess` (potrzebna do bootstrapu; codzienny deploy idzie przez CI).
3. Włącz MFA dla użytkownika.
4. Dostęp z CLI bez długożyjących kluczy:

   ```bash
   aws login --profile matchday-admin   # AWS CLI >= 2.32, logowanie przez przeglądarkę
   export AWS_PROFILE=matchday-admin AWS_REGION=eu-central-1
   aws sts get-caller-identity
   ```

   Jeśli twoja wersja CLI nie ma `aws login`, utwórz access key dla `admin`, wymuś MFA (`aws sts get-session-token --serial-number ... --token-code ...`) i usuń klucz po bootstrapie.

## 3. Bootstrap (Terraform)

Tworzy: bucket stanu Terraform, dostawcę OIDC GitHub, role `dam-github-plan` i `dam-github-deploy`, permission boundary `dam-permissions-boundary` oraz budżety 5 USD i 20 USD z alertami e-mail.

```bash
cp infra/bootstrap/terraform.tfvars.example infra/bootstrap/terraform.tfvars
# uzupełnij budget_alert_email (i github_repository, jeśli inne niż dawidbis/AWS_DAM_SPA)
just bootstrap
```

Sprawdź plan przed `yes`. Na końcu Terraform wypisze `github_actions_variables`.

## 4. Przeniesienie stanu bootstrapu do S3

Pierwszy `apply` używa stanu lokalnego (bucket jeszcze nie istniał). Żeby nie zgubić stanu:

1. W `infra/bootstrap/versions.tf` odkomentuj blok `backend "s3"`.
2. Uruchom:

   ```bash
   terraform -chdir=infra/bootstrap init -migrate-state \
     -backend-config="bucket=$(terraform -chdir=infra/bootstrap output -raw state_bucket)"
   ```

3. Usuń lokalne `infra/bootstrap/terraform.tfstate*` (są w `.gitignore`) i zacommituj zmianę w `versions.tf`.

## 5. Zmienne w GitHub

Repo → Settings → Secrets and variables → Actions → **Variables** (to nie są sekrety, ARN-y ról nie dają dostępu bez tokenu OIDC):

| Zmienna | Skąd |
|---|---|
| `AWS_REGION` | `eu-central-1` |
| `AWS_PLAN_ROLE_ARN` | output `github_plan_role_arn` |
| `AWS_DEPLOY_ROLE_ARN` | output `github_deploy_role_arn` |
| `TF_STATE_BUCKET` | output `state_bucket` |

```bash
terraform -chdir=infra/bootstrap output github_actions_variables
```

Dopóki zmienne nie są ustawione, joby `plan` i `deploy` są pomijane, a CI działa normalnie.

## 6. Ochrona gałęzi `main`

Settings → Branches (lub Rulesets) → `main`:

- wymagany pull request,
- wymagane statusy: `Rust (fmt, clippy, test)`, `Angular (lint, test, build)`, `Terraform (fmt, validate, tflint)`, `Checkov (skan IaC)`, `Cargo Lambda (arm64)`.

Rola `dam-github-deploy` ufa wyłącznie tokenom z `refs/heads/main`, więc ochrona `main` jest jednocześnie ochroną deployu.

## 7. Pierwszy deploy

Merge do `main` uruchamia `deploy.yml`: CI → `terraform apply` → wywołanie `matchday-dam-dev-hello-world`. Lokalnie (jako admin):

```bash
just deploy
just invoke-hello Kibic   # {"message":"Hello, Kibic!","version":"0.1.0"}
```

**Etap 0 jest gotowy**, gdy merge do `main` sam wdraża funkcję w Ruście, a smoke test w Actions przechodzi.

## Hamulec kosztów

```bash
just destroy   # usuwa środowisko dev; bootstrap (stan, role, budżety) zostaje
```

Bucket stanu ma `prevent_destroy` i politykę `Deny s3:DeleteBucket`. Żeby go usunąć, trzeba świadomie zdjąć oba zabezpieczenia.
