# `pipeline-renditions` — miniatura i podgląd ze znakiem wodnym

Krok `scan-pipeline` po CDR. Z **wersji po rekonstrukcji** (`clean/staging/<id>`) tworzy dwie wersje w buckecie `renditions`:

| Klucz | Rozmiar | Dla kogo | Po co |
|---|---|---|---|
| `thumb/<assetId>.jpg` | dłuższy bok ≤ 400 px, JPEG q80 | A, B | kafelki galerii i kolejki publikacji zamiast ładowania pełnego pliku |
| `preview/<assetId>.jpg` | dłuższy bok ≤ 1200 px, JPEG q75, **znak wodny** | D (sponsorzy) | jedyna wersja, jaką widzi grupa D — nigdy oryginał |

| | |
|---|---|
| Funkcja AWS | `matchday-dam-dev-renditions` |
| Wyzwalacz | stan `Renditions` w `scan-pipeline` |
| Rola IAM | `dam-renditions` |
| Pamięć / timeout | 2048 MB / 120 s |
| Zmienne | `CLEAN_BUCKET`, `RENDITIONS_BUCKET` |
| Kod | [`src/main.rs`](src/main.rs), znak wodny w [`src/watermark.rs`](src/watermark.rs) |

## Kontrakt

Wejście: `StepInput` z `validation: VALID` (format) i `disarm: CLEAN`. Bez nich → `Err` (błąd konfiguracji maszyny).

Wyjście (`shared::pipeline::RenditionsOutcome`):

```json
{ "thumbnailKey": "thumb/0b5e….jpg", "previewKey": "preview/0b5e….jpg" }
```

Błąd → ponowienia (`step_retry`) → `SCAN_FAILED`. Asset bez miniatur nie zostanie oznaczony jako czysty (`finalize-clean` wymaga `renditions` w wejściu).

## Działanie

1. Odczyt `clean/staging/<id>` (plik już po CDR — renditions nigdy nie dotykają oryginału z kwarantanny).
2. Dekodowanie z tymi samymi limitami co CDR (20 000 px, 1 GB).
3. `fit(image, max)`: zmniejszenie tak, żeby dłuższy bok miał najwyżej `max` px, **bez powiększania** małych obrazów.
4. Na podgląd 1200 px nakładany jest znak wodny (`watermark::apply`).
5. Kodowanie obu wersji jako JPEG (RGB8).
6. Zapis do `renditions` z `Content-Type: image/jpeg` i `Cache-Control: private, max-age=300`.

## Znak wodny

- Tekst `KS MATCHDAY PODGLAD` narysowany wbudowaną czcionką bitmapową 5×7 (bez plików fontów i dodatkowych zależności).
- Skala dobierana do rozmiaru obrazu, napis powtarzany w przesuniętych rzędach na **całym** obrazie (nie da się go wyciąć kadrowaniem).
- Biały tekst z kryciem 110/255 i ciemnym cieniem 70/255 — czytelny na jasnym i ciemnym tle.
- Napis jest częścią pikseli, więc nie da się go zdjąć jak metadanych.

## Uprawnienia (`dam-renditions-main`)

| Akcja | Zasób |
|---|---|
| `s3:GetObject` | `clean/staging/*` |
| `s3:PutObject` | `renditions/thumb/*`, `renditions/preview/*` |

Polityka bucketu `renditions` pozwala zapisywać tylko tej roli, a czytać tylko `dam-assets-read`.

## Testy

`renders_small_jpegs_within_size_limits`, `preview_carries_the_watermark_and_thumbnail_does_not`, `keeps_small_images_small`, `rejects_unknown_formats_and_garbage`, `every_character_of_the_text_has_a_glyph`, `watermark_covers_the_whole_image`, `works_on_tiny_images`.
