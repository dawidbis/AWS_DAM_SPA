# `api-asset-delete` — `DELETE /assets/{assetId}`

Usunięcie assetu przez administratora: wszystkie kopie pliku (po CDR, w staging, miniatura, podgląd, pozostałość w kwarantannie) i rekord w tabeli `assets`. Assety zainfekowane i będące w przetwarzaniu są chronione.

| | |
|---|---|
| Funkcja AWS | `matchday-dam-dev-asset-delete` |
| Wyzwalacz | API Gateway, `DELETE /assets/{assetId}` |
| Grupy | A |
| Rola IAM | `dam-asset-delete` |
| Pamięć / timeout | 128 MB / 10 s |
| Zmienne | `ASSETS_TABLE`, `QUARANTINE_BUCKET`, `CLEAN_BUCKET`, `RENDITIONS_BUCKET` |
| Kod | [`src/main.rs`](src/main.rs), reguła `shared::catalog::can_delete` |

## Kiedy wolno usunąć

| Status | Usunięcie | Dlaczego |
|---|---|---|
| `CLEAN_DRAFT`, `PUBLISHED`, `ARCHIVED` | ✅ | decyzja redakcyjna |
| `REJECTED`, `SCAN_FAILED` | ✅ | sprzątanie odrzuconych |
| `UPLOADING`, `QUARANTINED`, `SCANNING` | ❌ 409 | pipeline wciąż pracuje na pliku; usunięcie w trakcie zostawiłoby osierocone obiekty |
| `INFECTED` | ❌ 409 | plik w `infected` i rekord to **dowód incydentu** (Object Lock 90 dni) |

## Działanie

1. Autoryzacja: tylko A; ID musi być UUID (inaczej 404 bez wywołań AWS).
2. `get_status` (odczyt silnie spójny); brak rekordu → 404.
3. `can_delete(status)`; nie → 409 z nazwą statusu.
4. Usunięcie obiektów (`asset_objects`), po kolei:
   - `clean/<assetId>`
   - `clean/staging/<assetId>`
   - `renditions/thumb/<assetId>.jpg`
   - `renditions/preview/<assetId>.jpg`
   - `quarantine/<assetId>`

   Usunięcie nieistniejącego klucza w S3 też się udaje, więc kolejność i powtórzenia są bezpieczne. Bucket `infected` celowo nie jest na liście.
5. `DeleteItem` z warunkiem `#status = :status` — status odczytany w kroku 2. Jeśli w międzyczasie się zmienił (np. ktoś opublikował szkic), rekord zostaje i odpowiedź to 409.
6. Log na poziomie `WARN`: `asset deleted` z `asset_id`, statusem i `sub` admina.

Buckety mają wersjonowanie, więc usunięcie tworzy znacznik usunięcia, a poprzednia wersja jest odzyskiwalna przez okres retencji wersji nieaktualnych (`clean` 30 dni, `renditions` 7 dni, `quarantine` 1 dzień).

## Uprawnienia (`dam-asset-delete-main`)

| Akcja | Zasób |
|---|---|
| `dynamodb:GetItem`, `dynamodb:DeleteItem` | `assets` |
| `s3:DeleteObject` | `clean/*`, `renditions/*`, `quarantine/*` |

Brak dostępu do `infected` i do odczytu plików.

## Odpowiedzi

| Kod | Kiedy |
|---|---|
| 200 | `{ assetId }` |
| 403 | nie A |
| 404 | brak assetu, niepoprawne ID |
| 409 | status nie pozwala usunąć albo zmienił się w trakcie |

## Frontend

Przycisk **Usuń** w galerii i w panelu admina (sekcje szkiców i nieudanych skanów), z potwierdzeniem w oknie przeglądarki (`DeleteAssetService`). Po sukcesie karta znika z listy bez przeładowania.

## Testy

`only_admins_can_delete`, `malformed_ids_never_reach_aws`, `removes_every_copy_of_the_asset` (lista obiektów zawiera wszystkie kopie i nie zawiera `infected`). E2E (sekcja „Usuwanie assetów” po scenariuszu 16): C → 403, usunięcie `INFECTED` → 409, A → 200, rekord i plik znikają.
