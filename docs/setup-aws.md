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

Wystarczy AWS CloudShell (ikona `>_` w konsoli, region eu-central-1) z doinstalowanym Terraformem, bez lokalnych narzędzi.

Bucket stanu jeszcze nie istnieje, więc pierwsze `apply` robimy ze stanem lokalnym. Zapewnia to plik `backend_override.tf` (jest w `.gitignore`):

```bash
cp infra/bootstrap/terraform.tfvars.example infra/bootstrap/terraform.tfvars
# uzupełnij budget_alert_email; dla innego repozytorium także github_repository,
# github_owner_id i github_repository_id (ID z GitHub API, patrz komentarze w pliku)
echo 'terraform {
  backend "local" {}
}' > infra/bootstrap/backend_override.tf
terraform -chdir=infra/bootstrap init
terraform -chdir=infra/bootstrap apply
```

Sprawdź plan przed `yes` (19 zasobów do utworzenia). Na końcu Terraform wypisze `github_actions_variables`.

## 4. Przeniesienie stanu bootstrapu do S3

```bash
BUCKET=$(terraform -chdir=infra/bootstrap output -raw state_bucket)
rm infra/bootstrap/backend_override.tf
terraform -chdir=infra/bootstrap init -migrate-state -backend-config="bucket=$BUCKET"
# na pytanie "Do you want to copy existing state to the new backend?" odpowiedz: yes
terraform -chdir=infra/bootstrap plan
# oczekiwany wynik: "No changes."
rm -f infra/bootstrap/terraform.tfstate infra/bootstrap/terraform.tfstate.backup
```

Kolejne zmiany w bootstrapie (`just bootstrap`) korzystają już ze stanu w S3.

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

## 8. Konta testowe (etap 1)

Samodzielna rejestracja jest wyłączona, konta zakłada administrator. Po deployu z Cognito (CloudShell lub lokalnie, jako admin):

```bash
terraform -chdir=infra/envs/dev init -backend-config="bucket=<TF_STATE_BUCKET>"
./scripts/create-user.sh ty+admin@gmail.com admin
./scripts/create-user.sh ty+staff@gmail.com staff
./scripts/create-user.sh ty+foto@gmail.com contributor
./scripts/create-user.sh ty+sponsor@gmail.com viewer
```

Gmail dostarcza adresy `ty+cokolwiek@gmail.com` do tej samej skrzynki, więc jedna skrzynka wystarczy na cztery konta. Każde dostaje mail z hasłem tymczasowym; przy pierwszym logowaniu ustawiasz własne. Adres aplikacji: output `frontend_url`.

Lokalny frontend (`npm start`) potrzebuje `frontend/public/config.json`: `just frontend-config`.

## 9. Skanowanie antywirusowe (etap 1)

1. Repo → Settings → Secrets and variables → Actions → **Variables** → `ALERT_EMAIL` = adres na alerty o zainfekowanych plikach.
2. Po deployu AWS wyśle mail „AWS Notification - Subscription Confirmation” — kliknij **Confirm subscription**, inaczej alerty nie dotrą.
3. Test EICAR (scenariusz 1): zapisz w pliku tekstowym standardowy ciąg testowy EICAR, nadaj plikowi rozszerzenie `.jpg` i wgraj go jako `+foto`. W ciągu ~1–2 min status assetu w DynamoDB zmieni się na `INFECTED`, plik trafi do bucketu `infected`, a na `ALERT_EMAIL` przyjdzie alert. Zwykłe zdjęcie dostanie `CLEAN_DRAFT` i trafi do `clean`.

Pierwszy skan po deployu jest wolniejszy (cold start: wczytanie bazy sygnatur). Komunikaty, których nie udało się przetworzyć 3 razy, trafiają do kolejki `matchday-dam-dev-scan-dlq`.

## 10. Galeria, publikacja i pobieranie (etap 1)

Test kryterium ukończenia etapu 1:

1. `+foto` (C) wgrywa zwykłe zdjęcie JPEG. W „Moich zgłoszeniach” status zmienia się z „Skanowanie” na „Czeka na publikację” (lista sama się odświeża).
2. `+admin` (A) otwiera „Administracja”, widzi podgląd zdjęcia i klika **Publikuj**.
3. `+staff` (B) otwiera „Galeria”, widzi zdjęcie i klika **Pobierz**. Link jest ważny 5 minut i wystawia go Lambda `assets-read` po sprawdzeniu grupy.

Grupa C nie widzi galerii ani nie pobiera oryginałów. Plik zainfekowany widzi w „Moich zgłoszeniach” tylko jako „Odrzucony”, bez szczegółów wykrycia (pełny status widzi A). Grupa D dostanie podglądy z watermarkiem w etapie 2.

## 11. Pipeline Step Functions (etap 2)

Od etapu 2 plik z kwarantanny przechodzi przez maszynę stanów `matchday-dam-dev-scan-pipeline` (konsola AWS → Step Functions). Każde wykonanie nazywa się jak asset, więc łatwo je znaleźć i prześledzić krok po kroku.

1. Test EICAR jak w kroku 9 (plik `.jpg`): wykonanie przechodzi `MarkScanning → Scan → HandleInfected`. W DynamoDB pojawia się wpis w tabeli `matchday-dam-dev-incidents` (z adresem IP uploadu), a mail przychodzi z reguły EventBridge `asset.infected`.
2. Zwykłe zdjęcie: `MarkScanning → Scan → Validate → Disarm → FinalizeClean`, status „Czeka na publikację”. W galerii jest wersja po CDR: bez EXIF/XMP; autor, prawa autorskie i data wykonania trafiają do rekordu w DynamoDB (`exifArtist`, `exifCopyright`, `exifTakenAt`).
3. Plik o typie niezgodnym z deklaracją (np. zwykły tekst zapisany jako `.jpg`) albo obraz, którego nie da się zdekodować, kończy się `REJECTED` z powodem w `rejectReason`. Autor widzi go jako „Odrzucony”.
4. Błąd skanu kończy się statusem `SCAN_FAILED` (fail closed). Plik widać w „Administracja → Błędy skanu”, gdzie można ponowić skan, dopóki plik jest w kwarantannie (7 dni).

Bucket `infected` ma Object Lock w trybie GOVERNANCE (retencja 90 dni): nikt nie podmieni ani nie usunie dowodu bez uprawnienia `s3:BypassGovernanceRetention`.

## 12. Podglądy ze znakiem wodnym (etap 2)

Po CDR krok `Renditions` zapisuje w buckecie `matchday-dam-dev-renditions` miniaturę (`thumb/<id>.jpg`, 400 px) i podgląd ze znakiem wodnym (`preview/<id>.jpg`, 1200 px).

1. Wgraj i opublikuj nowe zdjęcie (wykonanie przechodzi `… → Disarm → Renditions → FinalizeClean`).
2. `+staff` w „Galerii” widzi miniaturę i może pobrać oryginał.
3. `+sponsor` (D) w „Galerii” widzi ten sam asset jako podgląd z napisem „KS MATCHDAY PODGLAD”, bez przycisku „Pobierz”. Assety opublikowane przed tym krokiem nie mają podglądów, więc D ich nie widzi.

## Hamulec kosztów

```bash
just destroy   # usuwa środowisko dev; bootstrap (stan, role, budżety) zostaje
```

Bucket stanu ma `prevent_destroy` i politykę `Deny s3:DeleteBucket`. Żeby go usunąć, trzeba świadomie zdjąć oba zabezpieczenia.
