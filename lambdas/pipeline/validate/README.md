# `pipeline-validate` — typ pliku i limity obrazu

Krok `scan-pipeline` po skanie antywirusowym, przed CDR. **Zero zaufania do klienta**: typ pliku ustala z magic bytes początku pliku, a deklaracja z `upload-init` służy wyłącznie do porównania. Wymiary obrazu czyta z nagłówka, bez dekodowania pikseli, więc bomba dekompresyjna zostaje odrzucona, zanim cokolwiek ją rozpakuje.

| | |
|---|---|
| Funkcja AWS | `matchday-dam-dev-validate` |
| Wyzwalacz | stan `Validate` w `scan-pipeline` |
| Rola IAM | `dam-validate` |
| Pamięć / timeout | 256 MB / 30 s |
| Zmienne | `ASSETS_TABLE`, `QUARANTINE_BUCKET` |
| Kod | [`src/main.rs`](src/main.rs), limity w `shared::pipeline::limits` |

## Kontrakt

Wejście: `StepInput` z `assetId` i `scan` (CLEAN).

Wyjście (`shared::pipeline::ValidationOutcome`):

```json
{ "result": "VALID", "detectedType": "image/jpeg", "width": 6000, "height": 4000, "sizeBytes": 18874368 }
{ "result": "REJECTED", "reason": "typ image/png niezgodny z deklaracją image/jpeg" }
```

`REJECTED` → stan `ValidationRejected` → `MarkRejected` (status `REJECTED`, powód w `rejectReason` z prefiksem `validate: `). Błąd (`Err`, np. awaria S3) → ponowienia, potem `SCAN_FAILED`.

## Działanie

1. `declaredContentType` z rekordu assetu (`GetItem`, odczyt silnie spójny).
2. **Pierwszy 1 MB pliku** z kwarantanny (`GetObject` z `Range: bytes=0-1048575`). 1 MB, a nie kilkadziesiąt KB, bo w JPEG nagłówek z wymiarami (SOF) bywa za segmentami APP z miniaturą EXIF i XMP. Rozmiar całego obiektu z nagłówka `Content-Range`.
3. Decyzja `validate(header, declared, size)` — kolejne warunki, pierwszy niespełniony daje `REJECTED`:

| # | Sprawdzenie | Przykład odrzucenia | Scenariusz |
|---|---|---|---|
| 1 | plik niepusty | `pusty plik` | — |
| 2 | rozmiar ≤ 200 MB (`MAX_IMAGE_BYTES`) | `plik większy niż 200 MB` | — |
| 3 | `infer` rozpoznaje typ po magic bytes | `nierozpoznany typ pliku` (np. SVG, HTML, tekst) | 3 |
| 4 | typ na liście `ALLOWED_CONTENT_TYPES` (JPEG, PNG, WebP) | `typ application/x-msdownload spoza dozwolonych` | 4 |
| 5 | typ = deklaracja | `typ image/png niezgodny z deklaracją image/jpeg` | 4 |
| 6 | `imagesize` odczytuje wymiary z nagłówka | `nie można odczytać wymiarów obrazu z nagłówka` | — |
| 7 | wymiary niezerowe | `obraz bez wymiarów` | — |
| 8 | bok ≤ 20 000 px, piksele ≤ 100 MP | `obraz 100000×100000 przekracza limit wymiarów` | 6 |

Dlaczego limity wymiarów? Obraz 100 000 × 100 000 px może mieć kilkaset bajtów jako PNG, ale po rozpakowaniu zająłby 40 GB pamięci. 100 MP jako RGBA to ok. 400 MB, co mieści się w limicie dekodera CDR (1 GB). Te same limity egzekwuje ponownie dekoder w `cdr` i `renditions` (obrona w głąb).

## Uprawnienia (`dam-validate-main`)

| Akcja | Zasób |
|---|---|
| `s3:GetObject` | `quarantine/*` |
| `dynamodb:GetItem` | `assets` |

Funkcja nie zmienia statusu (robi to stan `MarkRejected` maszyny) i niczego nie zapisuje.

## Testy

`accepts_a_png_matching_its_declaration`, `rejects_an_executable_renamed_to_jpg`, `rejects_svg_with_script`, `rejects_a_type_that_differs_from_the_declaration`, `rejects_decompression_bombs_before_decoding`, `rejects_empty_and_oversized_files`, `photo_and_exif_payload_pass_validation`, `attack_files_are_rejected` (pliki z `tests/security-fixtures/`).
