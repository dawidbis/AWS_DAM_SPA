# Testy — Matchday DAM

| Poziom | Gdzie | Uruchamia | Co sprawdza |
|---|---|---|---|
| Jednostkowe Rust | `lambdas/**/src/*.rs` (`#[cfg(test)]`) | `cargo test`, CI przy każdym PR | logika handlerów bez AWS: autoryzacja, walidacja, przejścia statusów, CDR, znak wodny, parsowanie zdarzeń i odpowiedzi clamd |
| Jednostkowe Angular | `frontend/src/**/*.spec.ts` | `npm test`, CI | serwisy, strażniki, komponenty, protokół uploadu |
| Statyczne IaC | `infra/` | `terraform validate`, `tflint`, Checkov, CI | poprawność i dobre praktyki Terraform |
| **E2E bezpieczeństwa** | [`e2e/run.sh`](e2e/run.sh) | workflow „E2E (rozdział 12)” (ręcznie) lub CloudShell | scenariusze ataków z rozdziału 12 na **żywym** środowisku dev |

## Pliki testowe (`security-fixtures/`)

Generowane skryptem [`security-fixtures/generate.py`](security-fixtures/generate.py) (bez zewnętrznych zależności), używane zarówno przez testy jednostkowe `validate`/`cdr`, jak i przez e2e.

| Plik | Co zawiera | Oczekiwany wynik pipeline'u |
|---|---|---|
| `photo.jpg` | zwykłe zdjęcie 640×480 (gradient) bez EXIF, w repo (baza dla pozostałych) | `CLEAN_DRAFT` |
| `exif-xss.jpg` | `photo.jpg` z `<script>` w polach EXIF `ImageDescription` i `Artist` | `CLEAN_DRAFT`, skrypt usunięty przez CDR |
| `polyglot.png` | poprawny PNG 32×32 z doklejonym HTML/JS po `IEND` | `CLEAN_DRAFT`, doklejona treść usunięta przez CDR |
| `svg-script.svg` | SVG z `<script>` | odrzucony przy `POST /uploads` (typ spoza listy); zadeklarowany jako PNG → `REJECTED` w `validate` |
| `exe-renamed.jpg` | nagłówek pliku wykonywalnego PE (`MZ`, „This program cannot be run in DOS mode”) z rozszerzeniem `.jpg` | `REJECTED` (magic bytes ≠ JPEG) |
| `bomb.png` | mały PNG z nagłówkiem 100 000 × 100 000 px | `REJECTED` na limicie wymiarów, przed dekodowaniem |

Plik EICAR (standardowy testowy „wirus”) skrypt e2e tworzy w locie, żeby nie trzymać go w repozytorium.

## E2E — scenariusze z rozdziału 12

Skrypt zakłada tymczasowych użytkowników w każdej grupie (A–D) przez `AdminCreateUser`, loguje ich klientem `e2e` (`AdminInitiateAuth`), wgrywa pliki tak jak przeglądarka (`POST /uploads` → `PUT` części → `complete`) i czeka na końcowy status w DynamoDB (domyślnie do 420 s, `E2E_TIMEOUT_S`).

| # | Scenariusz | Sprawdzenie |
|---|---|---|
| 1 | EICAR | `INFECTED`, plik w `infected`, wpis w `incidents` (+ mail na `ALERT_EMAIL`) |
| 2 | XSS w EXIF | `CLEAN_DRAFT`, pobrany plik po CDR nie zawiera ani `<script>`, ani segmentu `Exif` |
| 3 | SVG | 400 przy `upload-init`; zadeklarowany jako PNG → `REJECTED` |
| 4 | `.exe` jako `.jpg` | `REJECTED` |
| 5 | poliglota PNG+HTML | `CLEAN_DRAFT`, bez doklejonej treści |
| 6 | bomba dekompresyjna | `REJECTED`, `rejectReason` zawiera „limit” |
| 7 | `../../etc/passwd.jpg` | `CLEAN_DRAFT`, `originalFilename = passwd.jpg`, klucz S3 = UUID |
| 8 | `<script>` w tytule | 400 (JSON Schema) |
| 9 | C publikuje | 403 |
| 10 | D prosi o oryginał / galeria D | 403; w galerii tylko URL-e `/preview/` |
| 11 | wysłano więcej niż zadeklarowano | `complete` → 4xx, `REJECTED`, nic w kwarantannie |
| 12 | PUT na inny klucz niż w presigned URL | 403 z S3 (błąd podpisu) |
| 13 | podwójne zdarzenie S3 | drugie `StartExecution` z tą samą nazwą → `ExecutionAlreadyExists` |
| 14 | błąd skanera (rekord `SCANNING` bez pliku w kwarantannie) | `SCAN_FAILED` |
| 15 | role poza swoim zakresem | `iam:SimulatePrincipalPolicy` → Deny dla 6 przypadków, np. `dam-assets-read` nie czyta kwarantanny, `dam-scan` nie zapisuje do `clean`, `dam-cdr` nie zapisuje do `infected` (pomijany, jeśli brak uprawnienia do symulacji) |
| 16 | API bez tokenu / z tokenem innej puli | 401 |
| — | słowniki (etap 3) | D czyta słowniki bez sponsorów; C nie edytuje (403); mecz bez sezonu → 400; `<script>` w nazwie → 400; sezonu używanego przez mecz nie da się usunąć (409) |
| — | metadane (etap 3) | C → 403; nieznany zawodnik → 400; zainfekowany asset → 409; A opisuje asset → 200, sezon uzupełniony z meczu, tagi `E2E`/`e2e` zapisane jako jeden `e2e` |
| — | usuwanie | C → 403, `INFECTED` → 409, A → 200, rekord i plik znikają |

Na końcu skrypt usuwa użytkowników, assety i wpisy słowników z prefiksem przebiegu (pułapka `EXIT`, także po przerwaniu). Wyjątek: plik EICAR w `infected` zostaje (Object Lock) jako dowód. Test słowników zakłada własne wpisy (`<RUN_ID>-sezon`, `-mecz`, `-gracz`…), więc nie zależy od seeda, który A mógł zmienić.

### Uruchomienie

GitHub → Actions → **E2E (rozdział 12)** → Run workflow (gałąź `main`). Workflow przyjmuje rolę `dam-github-deploy`, która ma `iam:SimulatePrincipalPolicy` dla ról `dam-*` (po bootstrapie z kroku 13 w [`docs/setup-aws.md`](../docs/setup-aws.md)).

Lokalnie / CloudShell (poświadczenia administratora, zainicjalizowany `infra/envs/dev`):

```bash
./tests/e2e/run.sh
```

Wynik: lista `✔`/`✘` i kod wyjścia ≠ 0, jeśli którykolwiek scenariusz nie przeszedł.
