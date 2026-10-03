# Moduł `access-logs`

Wspólny bucket na logi dostępu: logi serwerowe S3 (wszystkie buckety plików i frontendu) oraz standardowe logi CloudFront. Przydają się przy analizie incydentów (kto pobierał co i kiedy, z jakiego IP).

## Co tworzy

| Zasób | Konfiguracja |
|---|---|
| `aws_s3_bucket.logs` | prywatny, SSE-S3, wersjonowanie; **sam nie loguje** (pętla logów o logach) |
| `aws_s3_bucket_ownership_controls` + `aws_s3_bucket_acl` | `BucketOwnerPreferred` + ACL z grantem dla właściciela i dla konta dostarczającego logi CloudFront (kanoniczne ID AWS) — standardowe logi CloudFront wymagają ACL na buckecie docelowym |
| `aws_s3_bucket_policy.logs` | zapis logów S3 przez `logging.s3.amazonaws.com` z tego konta, Deny bez TLS |
| `aws_s3_bucket_lifecycle_configuration` | obiekty `retention_days` (30), wersje 1 dzień, porzucone uploady 1 dzień |

Prefiksy: `s3/<bucket>/…` (logi S3), `cloudfront/<nazwa>/…` (CloudFront).

## Zmienne / outputs

Zmienne: `bucket_name`, `retention_days` (30). Outputs: `bucket_id`, `bucket_domain_name`.
