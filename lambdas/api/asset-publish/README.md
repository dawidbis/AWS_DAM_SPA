# `api-asset-publish` — `POST /assets/{assetId}/publish`

Publikacja czystego assetu przez administratora: `CLEAN_DRAFT` (albo `ARCHIVED`) → `PUBLISHED`. Od tej chwili asset jest widoczny w galerii dla grup A, B i D.

| | |
|---|---|
| Funkcja AWS | `matchday-dam-dev-asset-publish` |
| Wyzwalacz | API Gateway, `POST /assets/{assetId}/publish` |
| Grupy | A |
| Rola IAM | `dam-asset-publish` |
| Pamięć / timeout | 128 MB / 10 s |
| Zmienne | `ASSETS_TABLE` |
| Kod | [`src/main.rs`](src/main.rs) |

## Działanie

1. Autoryzacja: tylko grupa A.
2. ID musi być UUID (`is_asset_id`), inaczej 404 bez zapytania do DynamoDB.
3. `shared::assets::transition(…, Published, extra)`: jeden `UpdateItem` z warunkiem `#status IN (:CLEAN_DRAFT, :ARCHIVED)`, ustawiający `status`, `updatedAt`, `publishedAt` i `publishedBy` (`sub` admina).
4. Warunek niespełniony (asset zainfekowany, w kwarantannie, w skanowaniu, odrzucony albo nieistniejący) → **409**.

Nie ma osobnego sprawdzenia „czy plik jest czysty”: **jedyną drogą do `CLEAN_DRAFT` jest pozytywne zakończenie całego pipeline'u**, a warunek w DynamoDB nie pozwala ominąć tego statusu. Nie da się więc opublikować pliku, który nie przeszedł skanu, walidacji i CDR.

## Uprawnienia (`dam-asset-publish-main`)

| Akcja | Zasób |
|---|---|
| `dynamodb:UpdateItem` | `assets` |

Funkcja nie ma dostępu do S3: publikacja zmienia tylko status, a pliki są już w `clean` i `renditions`.

## Odpowiedzi

| Kod | Kiedy |
|---|---|
| 200 | `{ assetId, status: "PUBLISHED" }` |
| 403 | nie A |
| 404 | ID nie jest UUID |
| 409 | status nie pozwala na publikację |

## Testy

`only_admins_can_publish`, `malformed_ids_never_reach_dynamodb`. Dozwolone przejścia testuje `shared::status` (`infected_can_never_be_published`).
