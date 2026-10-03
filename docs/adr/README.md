# Architecture Decision Records

Format: krótki [MADR](https://adr.github.io/madr/) — kontekst, decyzja, konsekwencje. Nowy ADR: skopiuj [`template.md`](template.md), nadaj kolejny numer.

| Nr | Decyzja | Status |
|---|---|---|
| [0003](0003-terraform.md) | Terraform jako IaC | Zaakceptowany |
| [0005](0005-step-functions-scan-pipeline.md) | Step Functions do orkiestracji pipeline'u skanowania | Zaakceptowany |
| [0007](0007-cdr-reconstructed-images.md) | CDR i publikacja zrekonstruowanej wersji zamiast oryginału | Zaakceptowany |
| [0008](0008-fail-closed.md) | Fail closed przy błędach skanu | Zaakceptowany |
| [0012](0012-single-aws-account.md) | Jedno konto AWS w fazie Free Planu | Zaakceptowany |
| [0013](0013-github-oidc-deploy-roles.md) | GitHub OIDC, role plan/deploy i permission boundary | Zaakceptowany |
| [0014](0014-clamav-scanner-lambda.md) | Skaner ClamAV jako obraz kontenera Lambda z clamd | Zaakceptowany |

Pozostałe ADR-y z rozdziału 15 dokumentu projektu powstaną w etapach, w których zapadają decyzje.
