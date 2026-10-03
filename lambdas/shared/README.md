# `shared` — wspólna biblioteka Lambd

Jedyne źródło prawdy dla modeli domenowych i reguł, które musi stosować więcej niż jedna funkcja: statusy i ich przejścia, grupy i autoryzacja, reguły uploadu, widoczność w katalogu, kontrakty kroków pipeline'u. Z typów API generowane są typy TypeScript dla Angulara.

Kod: [`src/`](src/). Każdy moduł ma na górze komentarz `//!` z opisem i odwołaniem do rozdziału dokumentu projektu.

## Moduły

| Moduł | Zawartość | Używają |
|---|---|---|
| [`status`](src/status.rs) | `AssetStatus` (9 statusów), `allowed_predecessors`, `transition_to` | wszystkie |
| [`auth`](src/auth.rs) | `UserGroup` (A–D), `Caller`, `parse_groups` (claim `cognito:groups`), `require_any_group` | Lambdy API |
| [`http`](src/http.rs) | `ApiError` → kody HTTP, `respond`, `caller`, `source_ip`, `json_body`, `path_param`, `query_param`, `env`; feature `testing`: `request_as` | Lambdy API |
| [`upload`](src/upload.rs) | limity (200 MB, części 5/8 MiB, 10 000 części), `ALLOWED_CONTENT_TYPES`, JSON Schema `POST /uploads`, `InitUploadRequest::validate`, `part_size_for`, `check_parts` | upload-* , validate |
| [`multipart`](src/multipart.rs) | klient S3 bez zbędnych sum kontrolnych, `presign_parts`, `list_parts`, `complete`, `abort`, `quarantine_key` | upload-* |
| [`assets`](src/assets.rs) | klucz `ASSET#<id>`, `UploadSession`, `get_upload_session`, `get_status`, `transition`, `transition_idempotent`, `now_millis` | API i pipeline |
| [`catalog`](src/catalog.rs) | `AssetView`, `AssetSummary`, `AssetListResponse`, `DownloadResponse`, `AssetStatusResponse`, `AssetDeletedResponse`, `preview_source`, `watermark_only`, `status_for_uploader`, `can_download`, `can_delete`, `Cursor`, `is_asset_id`, `attachment_filename` | assets-read, asset-* |
| [`pipeline`](src/pipeline.rs) | `StepInput`, `ScanOutcome`, `ValidationOutcome`, `DisarmOutcome`, `RenditionsOutcome`, `PreservedMetadata`, `limits`, klucze (`thumbnail_key`, `preview_key`, `staging_key`), `execution_name`, `move_object`, `delete_object` | pipeline, asset-rescan, asset-delete |
| [`telemetry`](src/telemetry.rs) | logi JSON przez `tracing` (`RUST_LOG`, bez czasu i kolorów — CloudWatch dodaje własny czas) | wszystkie |

## Najważniejsze reguły

### Przejścia statusów (`status`)

| Do | Z (dozwolone poprzedniki) |
|---|---|
| `UPLOADING` | — (tylko utworzenie rekordu) |
| `QUARANTINED` | `UPLOADING` |
| `SCANNING` | `QUARANTINED`, `SCAN_FAILED` |
| `REJECTED` | `UPLOADING`, `SCANNING` |
| `INFECTED`, `SCAN_FAILED`, `CLEAN_DRAFT` | `SCANNING` |
| `PUBLISHED` | `CLEAN_DRAFT`, `ARCHIVED` |
| `ARCHIVED` | `PUBLISHED` |

`assets::transition` buduje z tej tabeli `ConditionExpression: #status IN (...)`. Ta sama tabela opisuje stany `dynamodb:updateItem` w maszynie Step Functions (`MarkScanning`, `MarkRejected`, `MarkScanFailed`).

`transition_idempotent` dodatkowo akceptuje sytuację, w której asset **już ma** status docelowy — kroki Step Functions mogą być ponawiane.

### Autoryzacja (`auth`, `http`)

- Autoryzator API Gateway sprawdził podpis, issuer i `client_id` tokenu; Lambda tylko czyta claimy.
- Brak `sub` → `MissingSubject` → 401 (fail closed). Brak grupy → `Forbidden` → 403.
- `parse_groups` obsługuje format API Gateway (`[admin staff]`), tablicę JSON i listę po przecinku; nieznane grupy pomija (wielkość liter ma znaczenie: `Admin` ≠ `admin`).

### Błędy API (`http::ApiError`)

| Wariant | Kod | Treść dla klienta |
|---|---|---|
| `BadRequest(msg)` | 400 | `msg` |
| `Unauthorized` | 401 | `Unauthorized` |
| `Forbidden` | 403 | `Forbidden` |
| `NotFound` | 404 | `Not found` |
| `Conflict(msg)` | 409 | `msg` |
| `Unprocessable(msg)` | 422 | `msg` |
| `Internal(detail)` | 500 | `Internal error` (szczegół tylko w logach) |

Każda odpowiedź ma `content-type: application/json` i `cache-control: no-store`.

### Kontrakty pipeline'u (`pipeline`)

Wejście każdego kroku to `StepInput`, który rośnie o wyniki kolejnych kroków:

```json
{
  "assetId": "0b5e…",
  "scan":       { "verdict": "CLEAN", "engine": "ClamAV …" },
  "validation": { "result": "VALID", "detectedType": "image/jpeg", "width": 6000, "height": 4000, "sizeBytes": 18874368 },
  "disarm":     { "result": "CLEAN", "sizeBytes": 6123456, "sha256": "…", "metadata": { "artist": "…" } },
  "renditions": { "thumbnailKey": "thumb/0b5e….jpg", "previewKey": "preview/0b5e….jpg" }
}
```

Pola `verdict` i `result` rozgałęziają maszynę stanów (warunki JSONata w `infra/modules/scanner/pipeline.tf`). Zmiana nazw wariantów wymaga zmiany maszyny.

`checked_asset_id()` weryfikuje format UUID, zanim ID stanie się kluczem S3.

`move_object` = `CopyObject` + `DeleteObject`, idempotentnie: brak źródła przy istniejącym celu oznacza udaną poprzednią próbę. Istnienie celu sprawdza `ListObjectsV2` (wymaga tylko `s3:ListBucket`), bo polityki bucketów blokują rolom pipeline'u `GetObject`/`HeadObject` poza ich zakresem.

### Limity (`upload`, `pipeline::limits`)

| Stała | Wartość |
|---|---|
| `MAX_UPLOAD_BYTES` = `MAX_IMAGE_BYTES` | 200 MiB |
| `MIN_PART_BYTES` / `DEFAULT_PART_BYTES` / `MAX_PARTS` | 5 MiB / 8 MiB / 10 000 |
| `MAX_TITLE_CHARS` / `MAX_FILENAME_CHARS` | 120 / 200 |
| `MAX_DIMENSION` / `MAX_PIXELS` | 20 000 px / 100 000 000 |
| `PART_URL_TTL` | 1 h |

Schemat [`schemas/upload-init.schema.json`](schemas/upload-init.schema.json) jest wkompilowany (`include_str!`) i walidowany przy każdym `POST /uploads`. Test `schema_limits_match_rust_constants` pilnuje, że limity w schemacie są zgodne ze stałymi Rusta.

## Typy TypeScript

Typy z `#[cfg_attr(test, derive(ts_rs::TS), ts(export))]` (`AssetStatus`, `AssetView`, `AssetSummary`, `AssetListResponse`, `DownloadResponse`, `AssetStatusResponse`, `AssetDeletedResponse`) są eksportowane przy `cargo test` do `frontend/src/app/core/api/generated-types/` (katalog ustawia `lambdas/.cargo/config.toml`). CI (krok „Typy TypeScript z Rusta są aktualne”) failuje, jeśli wygenerowane pliki różnią się od tych w repo — po zmianie typu trzeba uruchomić `cargo test` i zacommitować wynik.

## Feature `testing`

`shared = { workspace = true, features = ["testing"] }` w `[dev-dependencies]` daje `shared::http::testing::request_as(sub, groups)` — żądanie API Gateway z claimami JWT do testów handlerów.
