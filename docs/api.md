# API — Matchday DAM

HTTP API Gateway (`matchday-dam-dev`), bazowy adres w output Terraform `api_url` i w `config.json` frontendu (`apiUrl`). Wszystkie trasy:

- wymagają nagłówka `Authorization: Bearer <access_token>` z puli Cognito projektu (autoryzator JWT odrzuca resztę kodem **401** bez wywołania Lambdy),
- przyjmują i zwracają JSON (`content-type: application/json`, `cache-control: no-store`),
- zwracają błędy jako `{ "message": "..." }`; błąd 500 zawsze ma treść `Internal error` (szczegóły tylko w logach CloudWatch),
- podlegają throttlingowi 10 żądań/s (burst 20) na trasę; przekroczenie = **429** z API Gateway.

Typy odpowiedzi są zdefiniowane w Ruście (`lambdas/shared/src/catalog.rs`) i generowane do TypeScriptu (`frontend/src/app/core/api/generated-types/`) przez `ts-rs`.

| Metoda i ścieżka | Lambda | Grupy | Opis |
|---|---|---|---|
| `GET /me` | [`api-me`](../lambdas/api/me/README.md) | każda zalogowana | tożsamość i grupy |
| `POST /uploads` | [`upload-init`](../lambdas/api/upload-init/README.md) | A, C | rozpoczęcie uploadu |
| `GET /uploads/{assetId}` | [`upload-status`](../lambdas/api/upload-status/README.md) | A, C (autor) | stan uploadu, wznowienie |
| `POST /uploads/{assetId}/complete` | [`upload-complete`](../lambdas/api/upload-complete/README.md) | A, C (autor) | zakończenie uploadu |
| `GET /assets?view=…` | [`assets-read`](../lambdas/api/assets-read/README.md) | zależnie od widoku | listy assetów |
| `GET /assets/{assetId}/download` | [`assets-read`](../lambdas/api/assets-read/README.md) | A, B | link do pobrania |
| `POST /assets/{assetId}/publish` | [`asset-publish`](../lambdas/api/asset-publish/README.md) | A | publikacja |
| `POST /assets/{assetId}/rescan` | [`asset-rescan`](../lambdas/api/asset-rescan/README.md) | A | ponowienie skanu |
| `DELETE /assets/{assetId}` | [`asset-delete`](../lambdas/api/asset-delete/README.md) | A | usunięcie assetu |

Grupy: A = `admin`, B = `staff`, C = `contributor`, D = `viewer` (rozdział 14 w [`architecture.md`](architecture.md)).

---

## `GET /me`

Zwraca to, co backend widzi w tokenie. Służy do sprawdzenia łańcucha SPA → autoryzator → Lambda.

```json
200 OK
{ "sub": "3c4f…", "email": "foto@example.com", "groups": ["contributor"] }
```

## `POST /uploads`

Zakłada upload multipart w kwarantannie i zwraca presigned URL-e do wszystkich części.

Żądanie (walidowane [JSON Schema](../lambdas/shared/schemas/upload-init.schema.json), nieznane pola są błędem):

```json
{
  "filename": "mecz-01.jpg",
  "size": 18874368,
  "contentType": "image/jpeg",
  "title": "Gol w 90. minucie"
}
```

| Pole | Reguła |
|---|---|
| `filename` | 1–255 znaków, bez znaków sterujących; zapisywany tylko basename (`../../a.jpg` → `a.jpg`), max 200 znaków |
| `size` | 1 … 209 715 200 (200 MB) |
| `contentType` | `image/jpeg`, `image/png`, `image/webp` |
| `title` | opcjonalny, max 120 znaków, bez `<`, `>` i znaków sterujących |

Odpowiedź:

```json
201 Created
{
  "assetId": "0b5e6a1c-2f3d-4e5f-8a9b-0c1d2e3f4a5b",
  "partSize": 8388608,
  "partCount": 3,
  "parts": [
    { "partNumber": 1, "url": "https://…s3…amazonaws.com/0b5e…?partNumber=1&uploadId=…&X-Amz-Signature=…" },
    { "partNumber": 2, "url": "…" },
    { "partNumber": 3, "url": "…" }
  ],
  "urlsExpireInSeconds": 3600
}
```

Klient wysyła bajty `[(n-1)·partSize, n·partSize)` pliku metodą `PUT` na URL części `n` i zapamiętuje nagłówek `ETag` (CORS go odsłania). Bez nagłówka `Authorization`.

| Kod | Kiedy |
|---|---|
| 400 | niepoprawny JSON, pole niezgodne ze schematem (komunikat wskazuje pole, np. `Niepoprawne pole /contentType`), plik pusty lub za duży, typ spoza listy |
| 403 | wywołujący nie jest w grupie A ani C |

## `GET /uploads/{assetId}`

Stan uploadu do wznowienia po przerwaniu. Zwraca numery części zapisanych w S3 i **nowe** presigned URL-e tylko dla brakujących (stare mogły wygasnąć).

```json
200 OK
{
  "assetId": "0b5e…",
  "status": "UPLOADING",
  "partSize": 8388608,
  "partCount": 3,
  "uploadedParts": [1, 2],
  "parts": [{ "partNumber": 3, "url": "…" }],
  "urlsExpireInSeconds": 3600
}
```

Gdy upload jest już zakończony (`status` inny niż `UPLOADING`), `uploadedParts` zawiera wszystkie części, a `parts` jest puste.

| Kod | Kiedy |
|---|---|
| 403 | grupa inna niż A, C |
| 404 | asset nie istnieje **albo należy do innego użytkownika** (nie zdradzamy istnienia cudzych assetów) |

## `POST /uploads/{assetId}/complete`

Kończy upload. Rozmiar liczy z części zapisanych w S3 (`ListParts`), nie z deklaracji.

```json
200 OK
{ "assetId": "0b5e…", "status": "QUARANTINED" }
```

Ponowne wywołanie po sukcesie zwraca to samo (idempotencja). Po `QUARANTINED` pipeline startuje sam (zdarzenie S3); postęp widać w `GET /assets?view=mine`.

| Kod | Kiedy |
|---|---|
| 404 | asset nie istnieje albo nie należy do wywołującego |
| 409 | brakuje części (`Brakuje N części pliku; wznów upload`) albo upload nie jest już w toku |
| 422 | suma części ≠ deklarowany rozmiar albo część ma nieoczekiwany rozmiar; upload zostaje przerwany, asset dostaje `REJECTED` (`rejectReason = SIZE_MISMATCH`) |

## `GET /assets?view=<widok>[&cursor=<kursor>]`

| `view` | Grupy | Co zwraca | Podgląd (`previewUrl`) |
|---|---|---|---|
| `gallery` (domyślny) | A, B, D | `PUBLISHED` | A, B: miniatura 400 px; D: podgląd 1200 px ze znakiem wodnym (assety bez podglądu są pomijane) |
| `mine` | A, C | assety wywołującego (wszystkie statusy) | brak |
| `drafts` | A | `CLEAN_DRAFT` (kolejka publikacji) | miniatura |
| `failed` | A | `SCAN_FAILED` (do ponowienia) | brak |

```json
200 OK
{
  "items": [
    {
      "assetId": "0b5e…",
      "status": "PUBLISHED",
      "title": "Gol w 90. minucie",
      "originalFilename": "mecz-01.jpg",
      "contentType": "image/jpeg",
      "sizeBytes": 6123456,
      "createdAt": 1759500000000,
      "updatedAt": 1759500100000,
      "previewUrl": "https://…s3…/thumb/0b5e….jpg?X-Amz-Signature=…"
    }
  ],
  "nextCursor": "1759400000000.9f1c…"
}
```

- Sortowanie: od najnowszych (`createdAt` malejąco), 50 na stronę. `nextCursor: null` = ostatnia strona.
- `contentType` i `sizeBytes` po pipeline'ie pochodzą z serwera (magic bytes, rozmiar kopii po CDR); przed nim to deklaracja klienta.
- W widoku `mine` grupa C widzi `INFECTED` i `SCAN_FAILED` jako `REJECTED`, a `ARCHIVED` jest pominięty.
- `previewUrl` jest ważny 5 minut.

| Kod | Kiedy |
|---|---|
| 400 | nieznany widok, niepoprawny kursor |
| 403 | grupa nie ma dostępu do widoku |

## `GET /assets/{assetId}/download`

Presigned URL do pliku po CDR w buckecie `clean`, z `Content-Disposition: attachment; filename="<bezpieczna nazwa ASCII>"`.

```json
200 OK
{ "url": "https://…s3…/0b5e…?response-content-disposition=attachment…", "expiresInSeconds": 300 }
```

| Kod | Kiedy |
|---|---|
| 403 | grupa inna niż A, B (np. D) |
| 404 | asset nie istnieje, niepoprawne ID **albo status nie pozwala pobrać** (A: `PUBLISHED`, `CLEAN_DRAFT`; B: tylko `PUBLISHED`) |

## `POST /assets/{assetId}/publish`

`CLEAN_DRAFT` lub `ARCHIVED` → `PUBLISHED` (warunkowy zapis, zapisuje `publishedAt`, `publishedBy`).

```json
200 OK
{ "assetId": "0b5e…", "status": "PUBLISHED" }
```

| Kod | Kiedy |
|---|---|
| 403 | nie A |
| 404 | ID nie jest UUID |
| 409 | asset nie istnieje albo ma status, z którego nie wolno publikować (np. `INFECTED`, `QUARANTINED`) |

## `POST /assets/{assetId}/rescan`

`SCAN_FAILED` → `SCANNING` i nowe wykonanie `scan-pipeline`.

```json
202 Accepted
{ "assetId": "0b5e…", "status": "SCANNING" }
```

| Kod | Kiedy |
|---|---|
| 403 | nie A |
| 404 | ID nie jest UUID |
| 409 | asset nie ma statusu `SCAN_FAILED` |

## `DELETE /assets/{assetId}`

Usuwa pliki assetu (`clean`, `clean/staging`, `renditions`, `quarantine`) i rekord.

```json
200 OK
{ "assetId": "0b5e…" }
```

| Kod | Kiedy |
|---|---|
| 403 | nie A |
| 404 | asset nie istnieje lub ID nie jest UUID |
| 409 | status nie pozwala usunąć (`UPLOADING`, `QUARANTINED`, `SCANNING`, `INFECTED`) albo zmienił się w trakcie usuwania |

---

## Przykład z `curl`

```bash
API=$(terraform -chdir=infra/envs/dev output -raw api_url)
TOKEN=...   # access_token z przeglądarki (DevTools → Application → Session Storage) lub z tests/e2e

curl -s -H "Authorization: Bearer $TOKEN" "$API/me"
curl -s -H "Authorization: Bearer $TOKEN" "$API/assets?view=gallery" | jq '.items[].assetId'
```
