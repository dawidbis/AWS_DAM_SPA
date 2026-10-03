# `api-assets-read` — katalog i linki do plików

Jedyna funkcja API, która **czyta pliki z bucketów `clean` i `renditions`** — a dokładniej podpisuje do nich krótko żyjące presigned URL-e po sprawdzeniu uprawnień. Obsługuje dwie trasy:

- `GET /assets?view=gallery|mine|drafts|failed[&cursor=]` — listy assetów z podglądami,
- `GET /assets/{assetId}/download` — link do pobrania pliku (wersji po CDR).

| | |
|---|---|
| Funkcja AWS | `matchday-dam-dev-assets-read` |
| Wyzwalacz | API Gateway, obie trasy powyżej (rozróżnienie po parametrze ścieżki `assetId`) |
| Grupy | zależnie od widoku (tabela niżej); pobieranie A, B |
| Rola IAM | `dam-assets-read` |
| Pamięć / timeout | 256 MB / 10 s |
| Zmienne | `ASSETS_TABLE`, `CLEAN_BUCKET`, `RENDITIONS_BUCKET` |
| Kod | [`src/main.rs`](src/main.rs), reguły widoczności w [`shared/src/catalog.rs`](../../shared/src/catalog.rs) |

Grupy Cognito nie mają żadnego dostępu do S3. Polityka bucketów `clean` i `renditions` pozwala czytać obiekty tylko tej roli (oraz rolom pipeline'u w `clean`), więc każdy link do pliku przechodzi przez sprawdzenie uprawnień w tej funkcji.

## `GET /assets` — listy

### Widoki

| `view` | Grupy (`AssetView::allowed_groups`) | Zapytanie DynamoDB | Podgląd |
|---|---|---|---|
| `gallery` (domyślny) | A, B, D | `status-index`, `status = PUBLISHED` | A, B: `thumb/<id>.jpg`; D: `preview/<id>.jpg` ze znakiem wodnym |
| `mine` | A, C | `uploader-index`, `uploaderId = sub` | brak |
| `drafts` | A | `status-index`, `status = CLEAN_DRAFT` | `thumb/<id>.jpg` |
| `failed` | A | `status-index`, `status = SCAN_FAILED` | brak (plik leży w kwarantannie, której nigdy nie udostępniamy) |

### Działanie

1. Autoryzacja: `caller()` + `require_any_group(view.allowed_groups())`.
2. Parsowanie widoku (nieznany → 400) i kursora (`<createdAt>.<assetId>`, niepoprawny → 400) **przed** jakimkolwiek zapytaniem.
3. `Query` do właściwego indeksu, malejąco po `createdAt`, limit 50. Kursor staje się `ExclusiveStartKey`; wartość klucza partycji indeksu (status albo `sub`) bierzemy z widoku, nie z kursora, więc kursorem nie da się podejrzeć cudzych danych.
4. Dla każdego rekordu (`AssetRecord::from_item`, uszkodzone rekordy są pomijane):
   - **status widoczny** (`visible_status`): w widoku `mine` użytkownik bez grupy A widzi `INFECTED`/`SCAN_FAILED` jako `REJECTED`, a `ARCHIVED` jest pomijany (`status_for_uploader`);
   - **źródło podglądu** (`preview_source`): `Thumbnail` (A, B, asset z renditions), `Watermarked` (D), `Original` (A, B, stare assety sprzed kroku renditions — plik z `clean`), `None`;
   - grupa D w galerii nie widzi assetów bez podglądu ze znakiem wodnym (nigdy nie dostaje oryginału);
   - presigned GET (5 min) z wymuszonymi nagłówkami: `Content-Type`, `Content-Disposition: inline`, `Cache-Control: private, no-store`.
5. `nextCursor` z `LastEvaluatedKey`.

## `GET /assets/{assetId}/download` — pobieranie

1. Grupa A lub B, ID w formacie UUID (inaczej 404 bez zapytania do AWS).
2. `GetItem` (odczyt silnie spójny), `can_download(caller, status)`:
   - A: `PUBLISHED` i `CLEAN_DRAFT` (podgląd przed publikacją),
   - B: tylko `PUBLISHED`.
   Asset niedostępny wygląda jak nieistniejący (**404**).
3. Presigned GET do `clean/<assetId>` (5 min) z `Content-Disposition: attachment; filename="<nazwa>"`. Nazwę oczyszcza `attachment_filename`: tylko ASCII alfanumeryczne, `.`, `-`, `_`, spacja; bez wiodących kropek; max 120 znaków; pusta → `assetId`. To chroni przed wstrzyknięciem nagłówków i dziwnymi nazwami w systemie plików użytkownika.
4. Log `download url issued` z `asset_id` i `sub` (audyt pobrań, uzupełniany logami dostępu S3).

Plik w `clean` to zawsze **wersja po CDR**, nie oryginał od fotografa.

## Konfiguracja klienta S3

`ResponseChecksumValidation::WhenRequired` — bez tego SDK mógłby dopisać do podpisu nagłówek sumy kontrolnej, którego przeglądarka nie wyśle, i link nie działałby.

## Uprawnienia (`dam-assets-read-main`)

| Akcja | Zasób | Po co |
|---|---|---|
| `dynamodb:GetItem`, `dynamodb:Query` | `assets`, `assets/index/status-index`, `assets/index/uploader-index` | listy i pojedynczy rekord |
| `s3:GetObject` | `clean/*`, `renditions/*` | podpisywanie linków (presigned URL działa z uprawnieniami tej roli) |

## Odpowiedzi

| Kod | Kiedy |
|---|---|
| 200 | lista / link |
| 400 | nieznany widok, niepoprawny kursor |
| 401 | brak claimów |
| 403 | grupa bez dostępu do widoku; pobieranie przez C lub D |
| 404 | pobieranie: brak assetu, niepoprawne ID, status niedozwolony dla grupy |

## Testy

`mine_queries_only_the_callers_own_assets`, `contributors_see_infected_as_rejected_but_admins_see_the_truth`, `contributor_cannot_open_gallery`, `unknown_view_and_bad_cursor_are_rejected_before_any_query`, `contributor_and_viewer_cannot_download`, `download_url_is_short_lived_and_forces_attachment`. Reguły `preview_source`, `can_download`, `Cursor`, `attachment_filename` testuje `shared::catalog`.
