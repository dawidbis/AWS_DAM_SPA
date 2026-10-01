# 0012. Jedno konto AWS w fazie Free Planu

- Status: zaakceptowany
- Data: 2026-10-01

## Kontekst

Dobra praktyka to osobne konta dla zarządzania, logów, dev i prod w AWS Organizations, a dostęp ludzi przez IAM Identity Center. Projekt działa jednak na koncie z Free Planem (kredyty do 6 miesięcy), a dołączenie konta do organizacji kończy kredyty Free Tier.

## Decyzja

- Jedno konto, jeden region (`eu-central-1`), jedno środowisko `dev`.
- Ludzie: użytkownik IAM z MFA (root tylko do zadań wymagających roota). Dostęp z CLI przez `aws login`, bez długożyjących kluczy.
- Maszyny: role IAM przyjmowane przez OIDC (GitHub Actions) i role usług (Lambda, Step Functions).
- Separację, którą dałyby osobne konta, częściowo zastępuje permission boundary i prefiks `dam-` dla ról i polityk (ADR 0013).

## Rozważane alternatywy

- **Organizations + Identity Center + osobne konta** — docelowy kierunek, opisany w README jako rozwój; dziś koszt utraty kredytów.
- **Identity Center w trybie account instance** — nie daje dostępu do konta AWS przez permission sets, więc nie rozwiązuje problemu.

## Konsekwencje

- Kompromitacja roli deployu dotyka całego konta, ograniczona tylko przez boundary. Akceptowalne dla projektu edukacyjnego bez danych prawdziwych.
- Po zakończeniu Free Planu decyzja do rewizji (rozdział 11, etap 4).
