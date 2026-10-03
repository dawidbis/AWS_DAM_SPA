# 0008. Fail closed przy błędach skanu

- Status: zaakceptowany
- Data: 2026-10-03

## Kontekst

Atakujący może celowo wywołać błąd skanera, np. plikiem-bombą, uszkodzonym archiwum albo plikiem przekraczającym limity silnika. Jeśli błąd skanu oznaczałby „brak wykrycia”, taki plik trafiłby do galerii (rozdział 8: „fail open”).

## Decyzja

- Plik trafia dalej **tylko** przy jednoznacznym werdykcie `CLEAN`. Każdy inny wynik (błąd pobrania, błąd lub timeout clamd, `Heuristics.Limits.*`, wyjątek w kroku pipeline'u, nieznany werdykt) kończy się statusem `SCAN_FAILED`.
- `SCAN_FAILED` widzi tylko A. Plik zostaje w kwarantannie, której nikt poza rolami pipeline'u nie czyta, i znika z niej z lifecycle po kilku dniach.
- Autor zgłoszenia (C) widzi `SCAN_FAILED` i `INFECTED` jako „Odrzucony”, bez szczegółów (nie podpowiadamy, co zawiodło).
- A może ponowić skan (`SCAN_FAILED` → `SCANNING`), dopóki plik jest w kwarantannie.

## Konsekwencje

- Chwilowa awaria AWS lub skanera może zablokować uczciwy plik; wtedy A ponawia skan.
- Przejścia statusów są warunkowe (rozdział 5), więc nie da się przejść z `SCAN_FAILED` bezpośrednio do `CLEAN_DRAFT` ani `PUBLISHED`.
