# 0003. Terraform jako IaC

- Status: zaakceptowany
- Data: 2026-10-01

## Kontekst

Cała infrastruktura ma być odtwarzalna jednym poleceniem i przeglądana w pull requestach. Projekt jest edukacyjny i portfolio: liczy się czytelność kodu i to, że umiejętność przenosi się poza AWS.

## Decyzja

Terraform (HCL, provider `hashicorp/aws` 6.x), kod zgodny z OpenTofu.

- Stan w S3 z natywnym blokowaniem (`use_lockfile = true`, Terraform ≥ 1.10), bez tabeli DynamoDB na locki.
- Bucket stanu, OIDC i role CI w osobnym stosie `infra/bootstrap/`, wdrażanym ręcznie raz przez administratora. Środowiska (`infra/envs/dev`) wdraża wyłącznie CI.
- Nazwa bucketu zawiera ID konta i jest przekazywana przez `-backend-config`, więc kod nie zawiera identyfikatorów konta.
- Dokumenty polityk IAM przez `data "aws_iam_policy_document"`, nie wklejony JSON.
- Jakość: `terraform fmt`, `terraform validate`, `tflint` (ruleset AWS), Checkov z udokumentowanymi wyjątkami (`.checkov.yaml`).
- `.terraform.lock.hcl` w repozytorium, z sumami dla wszystkich platform.

## Rozważane alternatywy

- **AWS CDK** — wygodne pętle i typy, ale wiąże z AWS i CloudFormation, a diff jest mniej czytelny w PR.
- **AWS SAM** — dobre dla Lambd, słabe dla reszty (CloudFront, Cognito, IAM w szczegółach).
- **CloudFormation wprost** — rozwlekły YAML, brak modułów na poziomie Terraform.

## Konsekwencje

- Ręczne zmiany w konsoli są zakazane (rozjazd ze stanem).
- Bootstrap jest „kurą i jajkiem": pierwszy `apply` ma stan lokalny, potem migrujemy go do S3 (docs/setup-aws.md).
