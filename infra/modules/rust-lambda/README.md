# Moduł `rust-lambda`

Funkcja AWS Lambda w Ruście zbudowana przez Cargo Lambda (`bootstrap.zip`, runtime `provided.al2023`, **arm64**) z **własną, dedykowaną rolą IAM**. Używany przez wszystkie funkcje projektu poza `scan` (obraz kontenera, definiowany w module `scanner`).

## Co tworzy

| Zasób | Nazwa | Uwagi |
|---|---|---|
| `aws_iam_role.this` | `dam-<name>` | zaufanie tylko dla `lambda.amazonaws.com` z tego konta (`aws:SourceAccount`), permission boundary obowiązkowy |
| `aws_iam_role_policy_attachment.basic_execution` | — | `AWSLambdaBasicExecutionRole` (zapis logów) |
| `aws_iam_policy.this` | `dam-<name>-<klucz>` | jedna polityka customer managed na wpis w `policies` |
| `aws_iam_role_policy_attachment.extra` | — | dodatkowe gotowe polityki z `policy_arns` |
| `aws_cloudwatch_log_group.this` | `/aws/lambda/<function_name>` | retencja `log_retention_days` (14) |
| `aws_lambda_function.this` | `<function_name>` | logi JSON, `RUST_LOG` = poziom logów, `source_code_hash` z zipa |

## Zmienne

| Zmienna | Domyślnie | Opis |
|---|---|---|
| `name` | — | krótka nazwa (`upload-init`), `^[a-z0-9-]{1,48}$`; rola = `dam-<name>` |
| `function_name` | — | pełna nazwa funkcji (`matchday-dam-dev-upload-init`) |
| `description` | `""` | opis widoczny w konsoli |
| `zip_path` | — | `lambdas/target/lambda/<crate>/bootstrap.zip` |
| `permissions_boundary_arn` | — | `dam-permissions-boundary` |
| `policies` | `{}` | `klucz => dokument JSON` (zwykle `{ main = data.aws_iam_policy_document.x.json }`) |
| `policy_arns` | `{}` | dodatkowe ARN-y polityk |
| `memory_size` | 128 | MB |
| `timeout` | 10 | s |
| `reserved_concurrency` | -1 | limit współbieżności (-1 = bez limitu) |
| `log_level` | `INFO` | `TRACE`…`ERROR`, także `application_log_level` |
| `log_retention_days` | 14 | |
| `environment` | `{}` | zmienne funkcji (doklejane do `RUST_LOG`) |

Outputs: `function_name`, `function_arn`, `role_name`, `role_arn` (szczegóły w [`outputs.tf`](outputs.tf)).

## Przykład

```hcl
data "aws_iam_policy_document" "asset_publish" {
  statement {
    sid       = "TransitionAssetStatus"
    actions   = ["dynamodb:UpdateItem"]
    resources = [module.data.assets_table_arn]
  }
}

module "asset_publish" {
  source = "../../modules/rust-lambda"

  name                     = "asset-publish"
  function_name            = "${local.name_prefix}-asset-publish"
  zip_path                 = "${var.lambda_artifacts_dir}/api-asset-publish/bootstrap.zip"
  permissions_boundary_arn = data.aws_iam_policy.permissions_boundary.arn

  policies    = { main = data.aws_iam_policy_document.asset_publish.json }
  environment = { ASSETS_TABLE = module.data.assets_table_name }
}
```

## Dlaczego osobna rola na funkcję

Zasada najmniejszych uprawnień (rozdział 10.1 projektu): przejęcie jednej funkcji daje tylko jej uprawnienia. Np. `upload-init` może zapisać do kwarantanny, ale nie przeczyta z niej ani bajtu; `assets-read` czyta `clean`, ale nie zmieni statusu. Nazwy ról są stałe (`dam-<name>`), bo odwołują się do nich polityki bucketów w module `storage`.
