# Moduł `data` — DynamoDB

Trzy tabele: `assets` (stan każdego pliku), `incidents` (wykrycia malware) i `dictionaries` (słowniki klubu, etap 3). Wszystkie on-demand (`PAY_PER_REQUEST`), z Point-in-Time Recovery i szyfrowaniem kluczem należącym do AWS.

## `assets` — `<name_prefix>-assets`

| Klucz | Typ | Wartość |
|---|---|---|
| `pk` (HASH) | S | `ASSET#<assetId>` |

Indeksy globalne (projekcja `ALL`):

| Indeks | HASH | RANGE | Zapytania |
|---|---|---|---|
| `status-index` | `status` | `createdAt` (N) | galeria (`PUBLISHED`), kolejka publikacji (`CLEAN_DRAFT`), nieudane skany (`SCAN_FAILED`) |
| `uploader-index` | `uploaderId` | `createdAt` (N) | „moje zgłoszenia” |

Stan uploadu multipart (`uploadId`, `partSize`, `partCount`) jest zapisany przy assecie, więc osobna tabela `uploads` nie jest potrzebna. Pełna lista atrybutów i kto je zapisuje: [`docs/architecture.md` §12](../../../docs/architecture.md#12-tabele-dynamodb).

Statusy zmieniane są **wyłącznie warunkowymi zapisami** (`ConditionExpression` na obecnym statusie) — w Lambdach przez `shared::assets::transition`, w maszynie stanów przez zadania `dynamodb:updateItem`.

## `incidents` — `<name_prefix>-incidents`

| Klucz | Typ | Wartość |
|---|---|---|
| `incidentId` (HASH) | S | = `assetId` (jeden incydent na asset) |

Atrybuty: `assetId`, `uploaderId`, `sourceIp`, `signature`, `engine`, `detectedAt`, `status` (`OPEN`), `alertSentAt`. Zapisuje tylko `dam-handle-infected`. Panel incydentów w UI: etap 3.

## `dictionaries` — `<name_prefix>-dictionaries`

| Klucz | Typ | Wartość |
|---|---|---|
| `kind` (HASH) | S | `PLAYER`, `SEASON`, `COMPETITION`, `MATCH`, `SPONSOR` |
| `id` (RANGE) | S | slug nadany przez A, np. `michal-kruk`, `2025-26` |

Atrybut `data`: wpis jako JSON (`shared::dictionary`). Tabela ma kilkadziesiąt rekordów, więc lista to `Scan` (`dictionaries-read`) albo `Query` po `kind` (`dictionaries-write` sprawdza mecze przed usunięciem sezonu). Początkowe dane: [`scripts/seed/dictionaries.json`](../../../scripts/seed/dictionaries.json), wgrywane przez `scripts/seed-dictionaries.sh` tylko do pustej tabeli (krok deployu).

## Zmienne / outputs

Zmienne: `name_prefix`, `deletion_protection` (domyślnie `false` w dev). Outputs: `assets_table_name`, `assets_table_arn`, `incidents_table_name`, `incidents_table_arn`, `dictionaries_table_name`, `dictionaries_table_arn`.

## Przydatne zapytania

```bash
TABLE=$(terraform -chdir=infra/envs/dev output -raw assets_table)

# rekord assetu
aws dynamodb get-item --table-name "$TABLE" --key '{"pk":{"S":"ASSET#<id>"}}'

# assety czekające na publikację
aws dynamodb query --table-name "$TABLE" --index-name status-index \
  --key-condition-expression '#s = :s' \
  --expression-attribute-names '{"#s":"status"}' \
  --expression-attribute-values '{":s":{"S":"CLEAN_DRAFT"}}'
```
