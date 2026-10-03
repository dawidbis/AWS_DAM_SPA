# `api-upload-status` — `GET /uploads/{assetId}`

Pozwala **wznowić przerwany upload** (zamknięta karta, utrata sieci, wygasłe URL-e). Zwraca numery części, które S3 już przyjął, i nowe presigned URL-e wyłącznie dla brakujących. Źródłem prawdy o częściach jest S3 (`ListParts`), a nie przeglądarka.

| | |
|---|---|
| Funkcja AWS | `matchday-dam-dev-upload-status` |
| Wyzwalacz | API Gateway, `GET /uploads/{assetId}` |
| Grupy | A, C — i tylko autor uploadu |
| Rola IAM | `dam-upload-status` |
| Pamięć / timeout | 256 MB / 10 s |
| Zmienne | `ASSETS_TABLE`, `QUARANTINE_BUCKET` |
| Kod | [`src/main.rs`](src/main.rs) |

## Działanie

1. Autoryzacja: grupa A lub C.
2. `get_upload_session` czyta z `assets` (odczyt silnie spójny) `uploadId`, `partSize`, `partCount`, `status`, `uploaderId`.
3. **Własność**: jeśli `uploaderId ≠ sub` wywołującego → **404** (nie 403), żeby nie zdradzać istnienia cudzych assetów. Dotyczy także admina: wznowić upload może tylko jego autor.
4. Status inny niż `UPLOADING` (upload już zakończony lub odrzucony): odpowiedź z `uploadedParts = 1..N`, pustą listą `parts` i `urlsExpireInSeconds = 0`. Frontend wie wtedy, że nie ma czego wysyłać.
5. Status `UPLOADING`: `ListParts` w kwarantannie, wyliczenie brakujących numerów, presigned `UploadPart` dla każdego (ważne 1 h).

```json
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

Po stronie frontendu powiązanie pliku z `assetId` trzyma IndexedDB (`PendingUploadsStore`), więc po ponownym wybraniu tego samego pliku (nazwa, rozmiar, data modyfikacji) upload rusza od brakujących części.

## Uprawnienia (`dam-upload-status-main`)

| Akcja | Zasób | Po co |
|---|---|---|
| `s3:ListMultipartUploadParts` | `quarantine/*` | lista zapisanych części |
| `s3:PutObject` | `quarantine/*` | podpisywanie `UploadPart` dla brakujących części |
| `dynamodb:GetItem` | `assets` | stan uploadu |

## Odpowiedzi

| Kod | Kiedy |
|---|---|
| 200 | stan uploadu |
| 403 | grupa B lub D |
| 404 | asset nie istnieje albo należy do kogoś innego |
| 500 | błąd S3/DynamoDB (np. upload przerwany przez lifecycle po 2 dniach) |

## Testy

`owner_sees_own_upload`, `other_users_get_not_found`.
