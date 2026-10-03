# 0005. Step Functions do orkiestracji pipeline'u skanowania

- Status: zaakceptowany
- Data: 2026-10-03

## Kontekst

W etapie 1 jedna Lambda `scan` robiła wszystko: zmieniała statusy, skanowała, przenosiła plik do `clean` lub `infected` i wysyłała alert. Miała więc szerokie uprawnienia (zapis we wszystkich bucketach, SNS), a każdy nowy krok z etapu 2 (walidacja typu, CDR, renditions) rozbudowywałby tę jedną funkcję. Rozdział 7.1 wymaga, żeby każda funkcja robiła jedną rzecz i miała własną rolę, a kolejnością kroków, ponowieniami i błędami zajmowało się Step Functions.

## Decyzja

- **SQS → `start-scan` → Step Functions `scan-pipeline` (Standard).** `start-scan` tylko uruchamia wykonanie. Nazwa wykonania to ID assetu, więc powtórzone zdarzenie S3 kończy się `ExecutionAlreadyExists` zamiast drugim skanem (scenariusz 13). Kolejka zostaje jako bufor i DLQ (ADR 0004).
- **Kroki:** `MarkScanning` (QUARANTINED → SCANNING) → `Scan` (Lambda `scan`, tylko odczyt kwarantanny) → `FinalizeClean` albo `HandleInfected`. Zmiany statusów bez logiki (`MarkScanning`, `MarkScanFailed`) to bezpośrednia integracja z DynamoDB z `ConditionExpression`, bez dodatkowych Lambd.
- **Fail closed (ADR 0008):** każdy błąd, timeout albo niejednoznaczny werdykt prowadzi do `MarkScanFailed` (status `SCAN_FAILED`), a wykonanie kończy się stanem `Fail`.
- **Własne role kroków:** `dam-scan` (odczyt kwarantanny), `dam-finalize-clean` (zapis w `clean`), `dam-handle-infected` (zapis w `infected`, tabela `incidents`, `events:PutEvents`), `dam-scan-pipeline` (wywołanie tych trzech funkcji i zapis statusu).
- **Alert jako zdarzenie domenowe:** `handle-infected` publikuje `asset.infected` w EventBridge, a mail SNS wysyła reguła. Nowy odbiorca to nowa reguła, bez zmian w Lambdzie.
- **JSONata** jako język zapytań maszyny stanów (m.in. `$millis()` dla `updatedAt`).
- **Ponowienie przez admina:** `POST /assets/{id}/rescan` zmienia `SCAN_FAILED` → `SCANNING` i uruchamia wykonanie o nazwie `<id>-retry-<czas>` z flagą `marked`.

## Rozważane alternatywy

- **Rozbudowa jednej Lambdy** — prostsze, ale sprzeczne z zasadą jednej roli na funkcję i trudne do obserwowania.
- **Express Workflows** — tańsze przy dużym ruchu, ale bez gwarancji jednego wykonania na nazwę (idempotencja) i z limitem 5 minut; skan dużego pliku trwa dłużej.
- **EventBridge Pipes zamiast `start-scan`** — mniej kodu, ale nazwy wykonania nie da się ustawić na ID assetu, więc idempotencję trzeba by zapewniać inaczej.

## Konsekwencje

- Free Tier Step Functions to 4000 przejść stanów miesięcznie; jeden plik to ok. 6 przejść.
- Wykonanie przerwane timeoutem całej maszyny (bez `Catch`) zostawiłoby asset w `SCANNING`; wykrywanie takich assetów i alarmy na nieudane wykonania to etap 3.
- Kolejne kroki etapu 2 (`validate`, `cdr`, `renditions`) to nowe stany i Lambdy, bez zmian w istniejących krokach.
