# Step Functions scan-pipeline (rozdział 3.2) i Lambdy jego kroków.
#
#   SQS scan-queue → start-scan → wykonanie o nazwie = ID assetu (duplikat
#   zdarzenia S3 nie uruchomi drugiego, scenariusz 13)
#     MarkScanning   QUARANTINED → SCANNING (warunkowy zapis DynamoDB)
#     Scan           Lambda scan (ClamAV), wynik CLEAN / INFECTED / FAILED
#     FinalizeClean  plik do clean, CLEAN_DRAFT
#     HandleInfected plik do infected, INFECTED, incydent, asset.infected
#     MarkScanFailed każdy błąd lub timeout → SCAN_FAILED (fail closed)
#
# Logika biznesowa jest w Lambdach; maszyna stanów odpowiada za kolejność,
# ponowienia i obsługę błędów (rozdział 7.1). Zmiany statusów bez logiki
# (MarkScanning, MarkScanFailed) to bezpośrednia integracja z DynamoDB.

locals {
  start_scan_timeout_s = 30
  step_timeout_s       = 120
  event_source         = "matchday.dam"

  state_machine_name = "${var.name_prefix}-scan-pipeline"
  # ARN składany z nazwy: Lambdy znają go, zanim maszyna powstanie.
  state_machine_arn = "arn:${local.partition}:states:${local.region}:${local.account_id}:stateMachine:${local.state_machine_name}"
  default_bus_arn   = "arn:${local.partition}:events:${local.region}:${local.account_id}:event-bus/default"
}

# --- Lambdy kroków -----------------------------------------------------------------

data "aws_iam_policy_document" "start_scan" {
  statement {
    sid       = "ConsumeScanQueue"
    actions   = ["sqs:ReceiveMessage", "sqs:DeleteMessage", "sqs:GetQueueAttributes"]
    resources = [aws_sqs_queue.scan.arn]
  }

  statement {
    sid       = "StartScanPipeline"
    actions   = ["states:StartExecution"]
    resources = [local.state_machine_arn]
  }
}

module "start_scan" {
  source = "../rust-lambda"

  name                     = "start-scan"
  function_name            = "${var.name_prefix}-start-scan"
  description              = "Zdarzenie z kolejki skanowania uruchamia Step Functions scan-pipeline"
  zip_path                 = "${var.lambda_artifacts_dir}/pipeline-start-scan/bootstrap.zip"
  permissions_boundary_arn = var.permissions_boundary_arn
  timeout                  = local.start_scan_timeout_s
  log_retention_days       = var.log_retention_days

  policies = { main = data.aws_iam_policy_document.start_scan.json }

  environment = {
    STATE_MACHINE_ARN = local.state_machine_arn
    QUARANTINE_BUCKET = var.quarantine_bucket
  }
}

resource "aws_lambda_event_source_mapping" "start_scan" {
  count = local.create_lambda ? 1 : 0

  event_source_arn        = aws_sqs_queue.scan.arn
  function_name           = module.start_scan.function_arn
  batch_size              = 10
  function_response_types = ["ReportBatchItemFailures"]

  scaling_config {
    maximum_concurrency = 2
  }

  depends_on = [aws_sfn_state_machine.scan_pipeline]
}

data "aws_iam_policy_document" "finalize_clean" {
  statement {
    sid       = "MoveFromQuarantine"
    actions   = ["s3:GetObject", "s3:DeleteObject"]
    resources = ["${var.quarantine_bucket_arn}/*"]
  }

  statement {
    sid       = "WriteClean"
    actions   = ["s3:PutObject"]
    resources = ["${var.clean_bucket_arn}/*"]
  }

  # Idempotentne ponowienie: czy plik już jest w clean (ListObjectsV2).
  statement {
    sid       = "ListClean"
    actions   = ["s3:ListBucket"]
    resources = [var.clean_bucket_arn]
  }

  statement {
    sid       = "TransitionAsset"
    actions   = ["dynamodb:GetItem", "dynamodb:UpdateItem"]
    resources = [var.assets_table_arn]
  }
}

module "finalize_clean" {
  source = "../rust-lambda"

  name                     = "finalize-clean"
  function_name            = "${var.name_prefix}-finalize-clean"
  description              = "Krok scan-pipeline: czysty plik do clean, status CLEAN_DRAFT"
  zip_path                 = "${var.lambda_artifacts_dir}/pipeline-finalize-clean/bootstrap.zip"
  permissions_boundary_arn = var.permissions_boundary_arn
  memory_size              = 256
  timeout                  = local.step_timeout_s
  log_retention_days       = var.log_retention_days

  policies = { main = data.aws_iam_policy_document.finalize_clean.json }

  environment = {
    ASSETS_TABLE      = var.assets_table_name
    QUARANTINE_BUCKET = var.quarantine_bucket
    CLEAN_BUCKET      = var.clean_bucket
  }
}

data "aws_iam_policy_document" "handle_infected" {
  statement {
    sid       = "MoveFromQuarantine"
    actions   = ["s3:GetObject", "s3:DeleteObject"]
    resources = ["${var.quarantine_bucket_arn}/*"]
  }

  statement {
    sid       = "WriteInfected"
    actions   = ["s3:PutObject"]
    resources = ["${var.infected_bucket_arn}/*"]
  }

  statement {
    sid       = "ListInfected"
    actions   = ["s3:ListBucket"]
    resources = [var.infected_bucket_arn]
  }

  statement {
    sid       = "TransitionAsset"
    actions   = ["dynamodb:GetItem", "dynamodb:UpdateItem"]
    resources = [var.assets_table_arn]
  }

  statement {
    sid       = "RecordIncident"
    actions   = ["dynamodb:GetItem", "dynamodb:PutItem", "dynamodb:UpdateItem"]
    resources = [var.incidents_table_arn]
  }

  statement {
    sid       = "PublishAssetInfected"
    actions   = ["events:PutEvents"]
    resources = [local.default_bus_arn]

    condition {
      test     = "StringEquals"
      variable = "events:source"
      values   = [local.event_source]
    }
  }
}

module "handle_infected" {
  source = "../rust-lambda"

  name                     = "handle-infected"
  function_name            = "${var.name_prefix}-handle-infected"
  description              = "Krok scan-pipeline: plik do infected, incydent, zdarzenie asset.infected"
  zip_path                 = "${var.lambda_artifacts_dir}/pipeline-handle-infected/bootstrap.zip"
  permissions_boundary_arn = var.permissions_boundary_arn
  memory_size              = 256
  timeout                  = local.step_timeout_s
  log_retention_days       = var.log_retention_days

  policies = { main = data.aws_iam_policy_document.handle_infected.json }

  environment = {
    ASSETS_TABLE      = var.assets_table_name
    INCIDENTS_TABLE   = var.incidents_table_name
    QUARANTINE_BUCKET = var.quarantine_bucket
    INFECTED_BUCKET   = var.infected_bucket
  }
}

# --- Maszyna stanów ----------------------------------------------------------------

locals {
  # Ponowienia przy chwilowych błędach usług. Kroki są idempotentne.
  lambda_retry = [
    {
      ErrorEquals     = ["Lambda.ServiceException", "Lambda.AWSLambdaException", "Lambda.SdkClientException", "Lambda.TooManyRequestsException"]
      IntervalSeconds = 5
      MaxAttempts     = 4
      BackoffRate     = 2
      JitterStrategy  = "FULL"
    },
  ]
  step_retry = concat(local.lambda_retry, [
    {
      ErrorEquals     = ["States.TaskFailed"]
      IntervalSeconds = 10
      MaxAttempts     = 2
      BackoffRate     = 2
    },
  ])
  dynamo_retry = [
    {
      ErrorEquals     = ["DynamoDB.ProvisionedThroughputExceededException", "DynamoDB.ThrottlingException", "DynamoDB.InternalServerErrorException", "DynamoDB.RequestLimitExceeded"]
      IntervalSeconds = 2
      MaxAttempts     = 5
      BackoffRate     = 2
    },
  ]

  asset_key = { pk = { S = "{% 'ASSET#' & $states.input.assetId %}" } }
  to_scan_failed = {
    assetId = "{% $states.input.assetId %}"
    reason  = "{% $states.errorOutput.Error & ': ' & $states.errorOutput.Cause %}"
  }

  scan_pipeline = {
    Comment       = "Skan pliku z kwarantanny (rozdział 3.2). Każdy błąd kończy się SCAN_FAILED."
    QueryLanguage = "JSONata"
    StartAt       = "AlreadyMarked"
    States = {
      # Ponowienie przez admina (asset-rescan) samo ustawia SCANNING.
      AlreadyMarked = {
        Type    = "Choice"
        Choices = [{ Condition = "{% $exists($states.input.marked) and $states.input.marked = true %}", Next = "Scan" }]
        Default = "MarkScanning"
      }
      MarkScanning = {
        Type     = "Task"
        Resource = "arn:${local.partition}:states:::dynamodb:updateItem"
        Arguments = {
          TableName                = var.assets_table_name
          Key                      = local.asset_key
          UpdateExpression         = "SET #status = :scanning, updatedAt = :now"
          ConditionExpression      = "#status = :quarantined"
          ExpressionAttributeNames = { "#status" = "status" }
          ExpressionAttributeValues = {
            ":scanning"    = { S = "SCANNING" }
            ":quarantined" = { S = "QUARANTINED" }
            ":now"         = { N = "{% $string($millis()) %}" }
          }
        }
        Output = "{% $states.input %}"
        Retry  = local.dynamo_retry
        # Asset nie czeka na skan: duplikat albo plik bez rekordu.
        Catch = [{ ErrorEquals = ["DynamoDB.ConditionalCheckFailedException"], Next = "NotAwaitingScan" }]
        Next  = "Scan"
      }
      Scan = {
        Type     = "Task"
        Resource = "arn:${local.partition}:states:::lambda:invoke"
        Arguments = {
          FunctionName = local.create_lambda ? aws_lambda_function.scan[0].arn : ""
          Payload      = { assetId = "{% $states.input.assetId %}" }
        }
        Output         = { assetId = "{% $states.input.assetId %}", scan = "{% $states.result.Payload %}" }
        TimeoutSeconds = local.scan_timeout_s + 120
        Retry          = local.lambda_retry
        Catch          = [{ ErrorEquals = ["States.ALL"], Output = local.to_scan_failed, Next = "MarkScanFailed" }]
        Next           = "Verdict"
      }
      Verdict = {
        Type = "Choice"
        Choices = [
          { Condition = "{% $states.input.scan.verdict = 'CLEAN' %}", Next = "FinalizeClean" },
          { Condition = "{% $states.input.scan.verdict = 'INFECTED' %}", Next = "HandleInfected" },
        ]
        Default = "ScanNotConclusive"
      }
      ScanNotConclusive = {
        Type = "Pass"
        Output = {
          assetId = "{% $states.input.assetId %}"
          reason  = "{% 'scan: ' & ($exists($states.input.scan.reason) ? $states.input.scan.reason : 'brak werdyktu') %}"
        }
        Next = "MarkScanFailed"
      }
      FinalizeClean = {
        Type     = "Task"
        Resource = "arn:${local.partition}:states:::lambda:invoke"
        Arguments = {
          FunctionName = module.finalize_clean.function_arn
          Payload      = "{% $states.input %}"
        }
        Output         = "{% $states.result.Payload %}"
        TimeoutSeconds = local.step_timeout_s + 60
        Retry          = local.step_retry
        Catch          = [{ ErrorEquals = ["States.ALL"], Output = local.to_scan_failed, Next = "MarkScanFailed" }]
        End            = true
      }
      HandleInfected = {
        Type     = "Task"
        Resource = "arn:${local.partition}:states:::lambda:invoke"
        Arguments = {
          FunctionName = module.handle_infected.function_arn
          Payload      = "{% $states.input %}"
        }
        Output         = "{% $states.result.Payload %}"
        TimeoutSeconds = local.step_timeout_s + 60
        Retry          = local.step_retry
        Catch          = [{ ErrorEquals = ["States.ALL"], Output = local.to_scan_failed, Next = "MarkScanFailed" }]
        End            = true
      }
      MarkScanFailed = {
        Type     = "Task"
        Resource = "arn:${local.partition}:states:::dynamodb:updateItem"
        Arguments = {
          TableName                = var.assets_table_name
          Key                      = local.asset_key
          UpdateExpression         = "SET #status = :failed, scanVerdict = :verdict, scanError = :reason, updatedAt = :now"
          ConditionExpression      = "#status = :scanning"
          ExpressionAttributeNames = { "#status" = "status" }
          ExpressionAttributeValues = {
            ":failed"   = { S = "SCAN_FAILED" }
            ":scanning" = { S = "SCANNING" }
            ":verdict"  = { S = "FAILED" }
            ":reason"   = { S = "{% $substring($states.input.reason, 0, 500) %}" }
            ":now"      = { N = "{% $string($millis()) %}" }
          }
        }
        Retry = local.dynamo_retry
        Catch = [{ ErrorEquals = ["DynamoDB.ConditionalCheckFailedException"], Next = "ScanFailed" }]
        Next  = "ScanFailed"
      }
      ScanFailed = {
        Type  = "Fail"
        Error = "ScanFailed"
        Cause = "Plik nie przeszedł skanu, status SCAN_FAILED (fail closed)"
      }
      NotAwaitingScan = {
        Type    = "Succeed"
        Comment = "Duplikat zdarzenia albo asset nie czeka na skan"
      }
    }
  }
}

data "aws_iam_policy_document" "pipeline_trust" {
  statement {
    actions = ["sts:AssumeRole"]

    principals {
      type        = "Service"
      identifiers = ["states.amazonaws.com"]
    }

    condition {
      test     = "StringEquals"
      variable = "aws:SourceAccount"
      values   = [local.account_id]
    }
  }
}

resource "aws_iam_role" "pipeline" {
  name                 = "dam-scan-pipeline"
  description          = "Rola Step Functions scan-pipeline"
  assume_role_policy   = data.aws_iam_policy_document.pipeline_trust.json
  permissions_boundary = var.permissions_boundary_arn
}

data "aws_iam_policy_document" "pipeline" {
  #checkov:skip=CKV_AWS_111:Dostarczanie logów Step Functions i X-Ray wymagają Resource "*" (dokumentacja AWS)
  #checkov:skip=CKV_AWS_356:Dostarczanie logów Step Functions i X-Ray wymagają Resource "*" (dokumentacja AWS)
  statement {
    sid     = "InvokePipelineSteps"
    actions = ["lambda:InvokeFunction"]
    resources = concat(
      local.create_lambda ? [aws_lambda_function.scan[0].arn, "${aws_lambda_function.scan[0].arn}:*"] : [],
      [
        module.finalize_clean.function_arn, "${module.finalize_clean.function_arn}:*",
        module.handle_infected.function_arn, "${module.handle_infected.function_arn}:*",
      ],
    )
  }

  statement {
    sid       = "MarkAssetStatus"
    actions   = ["dynamodb:UpdateItem"]
    resources = [var.assets_table_arn]
  }

  statement {
    sid = "DeliverLogs"
    actions = [
      "logs:CreateLogDelivery",
      "logs:GetLogDelivery",
      "logs:UpdateLogDelivery",
      "logs:DeleteLogDelivery",
      "logs:ListLogDeliveries",
      "logs:PutResourcePolicy",
      "logs:DescribeResourcePolicies",
      "logs:DescribeLogGroups",
    ]
    resources = ["*"]
  }

  statement {
    sid       = "Tracing"
    actions   = ["xray:PutTraceSegments", "xray:PutTelemetryRecords", "xray:GetSamplingRules", "xray:GetSamplingTargets"]
    resources = ["*"]
  }
}

resource "aws_iam_policy" "pipeline" {
  name        = "dam-scan-pipeline-main"
  description = "Uprawnienia Step Functions scan-pipeline"
  policy      = data.aws_iam_policy_document.pipeline.json
}

resource "aws_iam_role_policy_attachment" "pipeline" {
  role       = aws_iam_role.pipeline.name
  policy_arn = aws_iam_policy.pipeline.arn
}

resource "aws_cloudwatch_log_group" "pipeline" {
  name              = "/aws/vendedlogs/states/${local.state_machine_name}"
  retention_in_days = var.log_retention_days
}

resource "aws_sfn_state_machine" "scan_pipeline" {
  count = local.create_lambda ? 1 : 0

  name       = local.state_machine_name
  type       = "STANDARD"
  role_arn   = aws_iam_role.pipeline.arn
  definition = jsonencode(local.scan_pipeline)

  # Wejście i wyjście kroków to tylko referencje i werdykty, więc pełne logi
  # wykonań są bezpieczne i pomagają w diagnozie.
  logging_configuration {
    log_destination        = "${aws_cloudwatch_log_group.pipeline.arn}:*"
    include_execution_data = true
    level                  = "ALL"
  }

  tracing_configuration {
    enabled = true
  }

  depends_on = [aws_iam_role_policy_attachment.pipeline]
}
