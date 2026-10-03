# `pipeline-finalize-clean` — zakończenie czystej ścieżki

Ostatni krok `scan-pipeline` dla pliku, który przeszedł skan, walidację, CDR i renditions. Zrekonstruowany plik staje się publikowalną kopią, oryginał od użytkownika znika z kwarantanny, a asset dostaje status `CLEAN_DRAFT` z danymi **ustalonymi przez pipeline**, nie przez klienta.

| | |
|---|---|
| Funkcja AWS | `matchday-dam-dev-finalize-clean` |
| Wyzwalacz | stan `FinalizeClean` w `scan-pipeline` |
| Rola IAM | `dam-finalize-clean` |
| Pamięć / timeout | 256 MB / 120 s |
| Zmienne | `ASSETS_TABLE`, `QUARANTINE_BUCKET`, `CLEAN_BUCKET` |
| Kod | [`src/main.rs`](src/main.rs) |

## Kontrakt

Wejście: pełny `StepInput` — `scan: CLEAN`, `validation: VALID`, `disarm: CLEAN`, `renditions`. **Brak któregokolwiek pozytywnego wyniku to błąd** (`clean_attributes` zwraca `Err`): krok uruchomiony bez kompletu oznaczałby błąd w definicji maszyny stanów, więc odmawiamy (fail closed).

Wyjście: `{ "assetId": "…", "status": "CLEAN_DRAFT" }` (koniec wykonania).

## Działanie

1. Zbudowanie listy atrybutów z wyników poprzednich kroków (przed jakąkolwiek zmianą w S3).
2. **`move_object`** `clean/staging/<id>` → `clean/<id>` (kopia + usunięcie źródła). Idempotentne: jeśli źródła już nie ma, a cel istnieje (poprzednia próba się udała), krok idzie dalej. Do sprawdzenia istnienia celu służy `ListObjectsV2`, bo polityka bucketu nie pozwala tej roli na `HeadObject` poza swoim zakresem.
3. **Usunięcie oryginału** z kwarantanny — nie jest już potrzebny, do galerii trafia wersja po CDR.
4. **`transition_idempotent`** `SCANNING → CLEAN_DRAFT` z atrybutami. Powtórzenie po sukcesie (status już `CLEAN_DRAFT`) nie jest błędem.

Zapisywane atrybuty:

| Atrybut | Źródło |
|---|---|
| `scanVerdict = CLEAN`, `scanEngine`, `scannedAt` | `scan` |
| `detectedType`, `width`, `height`, `originalSizeBytes` | `validate` (magic bytes, nagłówek) |
| `sizeBytes`, `sha256`, `disarmedAt` | `cdr` (rozmiar i hash **kopii po CDR**) |
| `exifArtist`, `exifCopyright`, `exifTakenAt` | `cdr` (whitelista EXIF, jeśli były) |
| `hasRenditions = true` | `renditions` |

Od tej chwili galeria pokazuje typ i rozmiar z serwera (`AssetRecord::from_item` woli `detectedType` i `sizeBytes` od deklaracji), a asset czeka w kolejce publikacji (`GET /assets?view=drafts`).

## Uprawnienia (`dam-finalize-clean-main`)

| Akcja | Zasób | Po co |
|---|---|---|
| `s3:GetObject`, `s3:DeleteObject` | `clean/staging/*` | źródło przeniesienia |
| `s3:PutObject` | `clean/*` | kopia docelowa |
| `s3:ListBucket` | `clean` | sprawdzenie, czy cel już istnieje (idempotencja) |
| `s3:DeleteObject` | `quarantine/*` | usunięcie oryginału |
| `dynamodb:GetItem`, `dynamodb:UpdateItem` | `assets` | zmiana statusu |

## Testy

`records_facts_established_by_the_pipeline`, `refuses_without_every_positive_result`.
