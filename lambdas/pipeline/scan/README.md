# `pipeline-scan` — skan antywirusowy ClamAV

Pierwszy krok maszyny `scan-pipeline`. Pobiera plik z kwarantanny do `/tmp` i skanuje go demonem ClamAV (`clamd`) uruchomionym wewnątrz środowiska Lambdy. Zwraca werdykt `CLEAN`, `INFECTED` (z nazwą sygnatury) albo `FAILED`. **Każdy błąd to `FAILED`** — fail closed (ADR 0008).

| | |
|---|---|
| Funkcja AWS | `matchday-dam-dev-scan` |
| Typ | **obraz kontenera** (ECR `matchday-dam-dev-scanner`), architektura **x86_64** |
| Wyzwalacz | stan `Scan` w `scan-pipeline` (`lambda:invoke`, synchronicznie) |
| Rola IAM | `dam-scan` |
| Pamięć / timeout / `/tmp` | 3008 MB / 600 s / 2048 MB |
| Zmienne | `QUARANTINE_BUCKET`, `CLAMD_CONFIG` (w obrazie), `RUST_LOG` |
| Kod | [`src/main.rs`](src/main.rs), demon i protokół: [`src/clamd.rs`](src/clamd.rs), obraz: [`container/`](container/) |
| Terraform | `infra/modules/scanner/main.tf` (`aws_lambda_function.scan`, ECR) |
| Decyzja | [ADR 0014](../../../docs/adr/0014-clamav-scanner-lambda.md) |

## Kontrakt

Wejście (z maszyny stanów): `{ "assetId": "<uuid>" }`

Wyjście (`shared::pipeline::ScanOutcome`):

```json
{ "verdict": "CLEAN",    "engine": "ClamAV 1.0.7/27780/..." }
{ "verdict": "INFECTED", "signature": "Eicar-Test-Signature", "engine": "ClamAV 1.0.7/..." }
{ "verdict": "FAILED",   "reason": "pobranie z kwarantanny: NoSuchKey ..." }
```

Maszyna stanów kieruje: `CLEAN` → `Validate`, `INFECTED` → `HandleInfected`, wszystko inne → `ScanNotConclusive` → `SCAN_FAILED`. Funkcja **nie zmienia statusu ani nie przenosi pliku** — robią to kolejne kroki z własnymi rolami. Ta rola może wyłącznie czytać kwarantannę.

## Działanie

1. `checked_asset_id()` — ID musi być UUID, inaczej `FAILED` bez dostępu do S3.
2. `GetObject` z kwarantanny → strumieniowy zapis do `/tmp/scan-<assetId>`.
3. `Clamd::ensure_started()`: przy pierwszym wywołaniu w nowym środowisku uruchamia `clamd` (wczytanie bazy sygnatur trwa kilkadziesiąt sekund, limit 3 min) i czeka na gniazdo `/tmp/clamd.sock`. W ciepłym środowisku demon już działa.
4. `SCAN <ścieżka>` przez gniazdo uniksowe (protokół clamd, komendy z prefiksem `z`, zakończone `\0`).
5. `parse_scan_response`:
   - `<ścieżka>: OK` → `Clean`,
   - `<ścieżka>: <sygnatura> FOUND` → `Infected(sygnatura)`,
   - `… Heuristics.Limits.Exceeded …`, `… ERROR`, cokolwiek nierozpoznanego → `Failed`.
6. Usunięcie pliku z `/tmp` (zawsze, także po błędzie).
7. `VERSION` — wersja silnika i bazy sygnatur do pola `engine` (trafia do rekordu assetu i incydentu).

Uchwyt procesu `clamd` siedzi za `Mutex`em. `ensure_started` przy każdym wywołaniu sprawdza, czy proces żyje (`try_wait`), i w razie potrzeby uruchamia go ponownie; proces jest zabijany razem ze środowiskiem (`kill_on_drop`).

## Obraz kontenera

[`container/Dockerfile`](container/Dockerfile):

- baza `debian:bookworm-slim` + pakiety `clamav`, `clamav-daemon`,
- **`freshclam` podczas budowania obrazu** — baza sygnatur jest wbudowana w obraz (w Lambdzie system plików jest tylko do odczytu, a pobieranie ~300 MB przy każdym cold starcie byłoby za wolne i zawodne),
- binarka `bootstrap` (ten crate) sama implementuje Lambda Runtime API (`lambda_runtime`), więc nie potrzebuje obrazu bazowego AWS,
- `USER nobody` — skaner nie potrzebuje roota, pisze tylko do `/tmp`.

[`container/clamd.conf`](container/clamd.conf): gniazdo w `/tmp`, `Foreground yes`, limity rozmiaru 1100 MB z `AlertExceedsMax yes` (przekroczenie limitu = alert `Heuristics.Limits.Exceeded` = `FAILED`, a nie „czysty”), `MaxThreads 2`, `ReadTimeout 600`.

**Aktualizacja sygnatur = przebudowa obrazu.** `deploy.yml` przebudowuje go:

- gdy zmienił się `lambdas/pipeline/scan`, `lambdas/shared` lub `Cargo.lock`,
- co tydzień (cron w poniedziałek 04:23 UTC) i przy ręcznym uruchomieniu workflow (`FORCE_SCANNER_BUILD`),
- gdy w ECR nie ma jeszcze obrazu.

Budowanie: [`scripts/build-scanner.sh`](../../../scripts/build-scanner.sh) (`cargo lambda build --release --x86-64`, `docker build`, push z tagiem `<sha>-<data>`; repozytorium ECR ma tagi niezmienne i trzyma 3 ostatnie obrazy). `--check` tylko decyduje, czy przebudowa jest potrzebna.

Dlaczego x86_64, skoro reszta Lambd to arm64? Obraz buduje się natywnie na runnerach GitHub (x86) bez emulacji QEMU, która przy ClamAV byłaby bardzo wolna.

## Uprawnienia (`dam-scan-main`)

| Akcja | Zasób |
|---|---|
| `s3:GetObject` | `quarantine/*` |

## Wydajność i koszty

- Cold start: ~30–60 s (wczytanie bazy do pamięci, ~1,2 GB RAM). Ciepłe wywołanie: sekundy.
- Brak reserved concurrency (limit konta = 10); liczbę równoległych skanów ogranicza `maximum_concurrency = 2` wyzwalacza `start-scan`.

## Testy

Testy `clamd.rs` (parsowanie odpowiedzi): `clean_file`, `eicar_is_infected`, `exceeded_limits_fail_closed`, `errors_and_garbage_fail_closed`, `unreachable_daemon_fails_closed`; `main.rs`: `maps_verdicts_to_pipeline_outcomes`. Prawdziwy skan EICAR: scenariusz e2e 1.
