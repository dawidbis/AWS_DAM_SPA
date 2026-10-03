# Moduł `scanner` — pipeline bezpieczeństwa

Wszystko, co dzieje się z plikiem po trafieniu do kwarantanny: kolejka zdarzeń, maszyna stanów Step Functions, sześć Lambd kroków (skan ClamAV, walidacja, CDR, renditions, finalizacja, obsługa infekcji) i alert e-mail.

```
S3 quarantine → EventBridge → SQS scan-queue (+ DLQ) → Lambda start-scan
  → Step Functions scan-pipeline → scan / validate / cdr / renditions / finalize-clean
                                  └→ handle-infected → zdarzenie asset.infected → SNS e-mail
```

Szczegółowy opis przepływu i diagram stanów: [`docs/architecture.md` §6](../../../docs/architecture.md#6-przepływ-3-pipeline-bezpieczeństwa-step-functions). Opis każdej funkcji: [`lambdas/pipeline/`](../../../lambdas/pipeline/).

## Pliki

| Plik | Zawartość |
|---|---|
| [`main.tf`](main.tf) | ECR, SQS + DLQ, reguła EventBridge `quarantine-object-created`, polityka kolejki, rola i funkcja `scan` (obraz kontenera) |
| [`pipeline.tf`](pipeline.tf) | Lambdy `start-scan`, `validate`, `cdr`, `renditions`, `finalize-clean`, `handle-infected` (moduł `rust-lambda`) z politykami; wyzwalacz SQS; definicja maszyny stanów (JSONata); rola i log group maszyny |
| [`alerts.tf`](alerts.tf) | temat SNS `security-alerts`, subskrypcja e-mail, reguła EventBridge `asset-infected` z `input_transformer`, polityka tematu |
| [`variables.tf`](variables.tf), [`outputs.tf`](outputs.tf) | |

## Zasoby

| Zasób | Nazwa | Konfiguracja |
|---|---|---|
| ECR | `<prefix>-scanner` | tagi niezmienne, skan przy pushu, AES256, 3 ostatnie obrazy |
| SQS | `<prefix>-scan-queue` | widoczność 180 s (6 × timeout `start-scan`), retencja 4 dni, SSE, redrive po 3 próbach |
| SQS | `<prefix>-scan-dlq` | retencja 14 dni |
| EventBridge | `<prefix>-quarantine-object-created` | `source = aws.s3`, `detail-type = Object Created`, `bucket.name = quarantine` → kolejka |
| Lambda | `<prefix>-scan` | obraz, x86_64, 3008 MB, 600 s, `/tmp` 2048 MB, rola `dam-scan` |
| Lambda | `<prefix>-start-scan` | 128 MB, 30 s; wyzwalacz SQS: paczka 10, `maximum_concurrency = 2`, `ReportBatchItemFailures` |
| Lambda | `<prefix>-validate` | 256 MB, 30 s |
| Lambda | `<prefix>-cdr` | 2048 MB, 120 s |
| Lambda | `<prefix>-renditions` | 2048 MB, 120 s |
| Lambda | `<prefix>-finalize-clean` | 256 MB, 120 s |
| Lambda | `<prefix>-handle-infected` | 256 MB, 120 s |
| Step Functions | `<prefix>-scan-pipeline` | Standard, JSONata, rola `dam-scan-pipeline`, logi `ALL` z danymi wykonania, X-Ray |
| CloudWatch Logs | `/aws/vendedlogs/states/<prefix>-scan-pipeline` | 14 dni |
| SNS | `<prefix>-security-alerts` | subskrypcja e-mail, gdy `alert_email` ≠ `""` |
| EventBridge | `<prefix>-asset-infected` | `source = matchday.dam`, `detail-type = asset.infected` → SNS (tekst przez `input_transformer`) |

## Maszyna stanów

Definicja to obiekt HCL `local.scan_pipeline` zamieniany przez `jsonencode` — dzięki temu ARN-y funkcji, nazwy tabel i polityki ponowień wstawia Terraform, a całość jest w jednym miejscu z uprawnieniami.

- **Język zapytań JSONata** (`QueryLanguage = "JSONata"`): warunki (`{% $states.input.scan.verdict = 'CLEAN' %}`), budowanie wyjść (`$merge([$states.input, {'validation': $states.result.Payload}])`) i wyrażenia w argumentach DynamoDB.
- **Zmiany statusu bez Lambdy**: `MarkScanning`, `MarkRejected`, `MarkScanFailed` to bezpośrednie integracje `arn:aws:states:::dynamodb:updateItem` z `ConditionExpression`.
- **Kroki Lambd** przez `arn:aws:states:::lambda:invoke` z `TimeoutSeconds` = timeout funkcji + zapas.
- **Ponowienia**: `lambda_retry` (błędy usługi Lambda, 4×, backoff 2, jitter), `step_retry` (+ `States.TaskFailed` 2×), `dynamo_retry` (throttling, 5×).
- **Catch** na każdym kroku Lambdy → `MarkScanFailed` z przyczyną `Error: Cause` (fail closed).

Pełna tabela stanów: [`docs/architecture.md` §6.2](../../../docs/architecture.md#62-maszyna-stanów-scan-pipeline).

## Warunkowe tworzenie (`image_uri`)

Funkcja `scan`, maszyna stanów i wyzwalacz `start-scan` powstają tylko, gdy `image_uri` nie jest pusty (`local.create_lambda`). Pierwszy deploy na świeżym koncie tworzy najpierw repozytorium ECR (`terraform apply -target`), potem `scripts/build-scanner.sh` buduje i wypycha obraz, a dopiero potem pełny `apply` tworzy resztę. ARN maszyny stanów jest wyliczany z nazwy (`local.state_machine_arn`), żeby polityki Lambd mogły się do niego odwoływać przed jej utworzeniem.

## Uprawnienia

| Rola | Uprawnienia |
|---|---|
| `dam-scan` | `s3:GetObject` quarantine/* |
| `dam-start-scan` | SQS receive/delete/attributes na `scan-queue`; `states:StartExecution` |
| `dam-validate` | `s3:GetObject` quarantine/*; `dynamodb:GetItem` assets |
| `dam-cdr` | `s3:GetObject` quarantine/*; `s3:PutObject` clean/staging/* |
| `dam-renditions` | `s3:GetObject` clean/staging/*; `s3:PutObject` renditions/thumb/*, preview/* |
| `dam-finalize-clean` | `s3:GetObject`/`DeleteObject` clean/staging/*; `s3:PutObject` clean/*; `s3:ListBucket` clean; `s3:DeleteObject` quarantine/*; `dynamodb:GetItem`/`UpdateItem` assets |
| `dam-handle-infected` | `s3:GetObject`/`DeleteObject` quarantine/*; `s3:PutObject` infected/*; `s3:ListBucket` infected; DynamoDB assets i incidents; `events:PutEvents` z warunkiem `events:source = matchday.dam` |
| `dam-scan-pipeline` | `lambda:InvokeFunction` (6 funkcji kroków i ich wersje); `dynamodb:UpdateItem` assets; dostarczanie logów; X-Ray |

Polityka kolejki: `sqs:SendMessage` tylko z reguły `quarantine-object-created` (`aws:SourceArn`), Deny bez TLS. Polityka tematu SNS: `sns:Publish` tylko z reguły `asset-infected`, Deny bez TLS.

## Zmienne

| Zmienna | Opis |
|---|---|
| `name_prefix` | `matchday-dam-dev` |
| `image_uri` | URI obrazu skanera w ECR; `""` = bez funkcji `scan` i maszyny stanów |
| `permissions_boundary_arn` | boundary dla ról |
| `alert_email` | adres alertów; `""` = bez subskrypcji |
| `lambda_artifacts_dir` | katalog `bootstrap.zip` |
| `assets_table_*`, `incidents_table_*` | nazwy i ARN-y tabel |
| `quarantine_bucket*`, `clean_bucket*`, `renditions_bucket*`, `infected_bucket*` | nazwy i ARN-y bucketów |
| `log_retention_days` | 14 |

Outputs: `repository_url`, `repository_name`, `scan_queue_url`, `dlq_url`, `alerts_topic_arn`, `state_machine_arn`.

## Koszty

Step Functions Standard: ~10 przejść stanów na plik (darmowe 4000/miesiąc). Lambda `scan` z 3 GB pamięci to główny koszt — cold start z wczytaniem bazy ClamAV, dlatego `maximum_concurrency = 2` ogranicza liczbę równoległych środowisk.
