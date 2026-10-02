# Pipeline skanowania etapu 1 (rozdział 3.1):
#   S3 quarantine → EventBridge → SQS scan-queue (+ DLQ) → Lambda scan (ClamAV)
# Lambda jest obrazem kontenera w ECR; obraz buduje CI (deploy.yml), więc
# funkcja powstaje dopiero, gdy podany jest image_uri.

terraform {
  required_version = ">= 1.10"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = ">= 6.0"
    }
  }
}

data "aws_caller_identity" "current" {}

data "aws_partition" "current" {}

locals {
  name           = "${var.name_prefix}-scan"
  create_lambda  = var.image_uri != ""
  scan_timeout_s = 600
}

# --- ECR ---------------------------------------------------------------------

resource "aws_ecr_repository" "this" {
  #checkov:skip=CKV_AWS_136:Szyfrowanie AES256 zamiast KMS CMK (koszt, rozdział 7.3)
  name                 = "${var.name_prefix}-scanner"
  image_tag_mutability = "IMMUTABLE"
  force_delete         = true

  image_scanning_configuration {
    scan_on_push = true
  }

  encryption_configuration {
    encryption_type = "AES256"
  }
}

# Stare obrazy (z nieaktualnymi sygnaturami) nie są potrzebne.
resource "aws_ecr_lifecycle_policy" "this" {
  repository = aws_ecr_repository.this.name
  policy = jsonencode({
    rules = [{
      rulePriority = 1
      description  = "Zostaw 3 najnowsze obrazy"
      selection = {
        tagStatus   = "any"
        countType   = "imageCountMoreThan"
        countNumber = 3
      }
      action = { type = "expire" }
    }]
  })
}

# --- Kolejka -------------------------------------------------------------------

resource "aws_sqs_queue" "dlq" {
  name                      = "${var.name_prefix}-scan-dlq"
  message_retention_seconds = 14 * 24 * 3600
  sqs_managed_sse_enabled   = true
}

resource "aws_sqs_queue" "scan" {
  name = "${var.name_prefix}-scan-queue"
  # AWS zaleca widoczność ≥ 6 × timeout funkcji przy wyzwalaczu SQS.
  visibility_timeout_seconds = 6 * local.scan_timeout_s
  message_retention_seconds  = 4 * 24 * 3600
  sqs_managed_sse_enabled    = true

  redrive_policy = jsonencode({
    deadLetterTargetArn = aws_sqs_queue.dlq.arn
    maxReceiveCount     = 3
  })
}

resource "aws_cloudwatch_event_rule" "object_created" {
  name        = "${var.name_prefix}-quarantine-object-created"
  description = "Nowy plik w kwarantannie → kolejka skanowania"
  event_pattern = jsonencode({
    source        = ["aws.s3"]
    "detail-type" = ["Object Created"]
    detail = {
      bucket = { name = [var.quarantine_bucket] }
    }
  })
}

resource "aws_cloudwatch_event_target" "queue" {
  rule = aws_cloudwatch_event_rule.object_created.name
  arn  = aws_sqs_queue.scan.arn
}

# Do kolejki pisze wyłącznie ta reguła EventBridge (rozdział 10.2).
data "aws_iam_policy_document" "queue" {
  statement {
    sid       = "AllowQuarantineRule"
    actions   = ["sqs:SendMessage"]
    resources = [aws_sqs_queue.scan.arn]

    principals {
      type        = "Service"
      identifiers = ["events.amazonaws.com"]
    }

    condition {
      test     = "ArnEquals"
      variable = "aws:SourceArn"
      values   = [aws_cloudwatch_event_rule.object_created.arn]
    }
  }

  statement {
    sid       = "DenyInsecureTransport"
    effect    = "Deny"
    actions   = ["sqs:*"]
    resources = [aws_sqs_queue.scan.arn]

    principals {
      type        = "*"
      identifiers = ["*"]
    }

    condition {
      test     = "Bool"
      variable = "aws:SecureTransport"
      values   = ["false"]
    }
  }
}

resource "aws_sqs_queue_policy" "scan" {
  queue_url = aws_sqs_queue.scan.id
  policy    = data.aws_iam_policy_document.queue.json
}

# --- Alerty --------------------------------------------------------------------

resource "aws_sns_topic" "alerts" {
  #checkov:skip=CKV_AWS_26:Alert zawiera tylko ID assetu i nazwę sygnatury; KMS CMK to dodatkowy koszt
  name = "${var.name_prefix}-security-alerts"
}

resource "aws_sns_topic_subscription" "email" {
  count = var.alert_email == "" ? 0 : 1

  topic_arn = aws_sns_topic.alerts.arn
  protocol  = "email"
  endpoint  = var.alert_email
}

# --- Lambda --------------------------------------------------------------------

data "aws_iam_policy_document" "trust" {
  statement {
    actions = ["sts:AssumeRole"]

    principals {
      type        = "Service"
      identifiers = ["lambda.amazonaws.com"]
    }

    condition {
      test     = "StringEquals"
      variable = "aws:SourceAccount"
      values   = [data.aws_caller_identity.current.account_id]
    }
  }
}

resource "aws_iam_role" "scan" {
  name                 = "dam-scan"
  description          = "Rola funkcji Lambda scan (ClamAV)"
  assume_role_policy   = data.aws_iam_policy_document.trust.json
  permissions_boundary = var.permissions_boundary_arn
}

resource "aws_iam_role_policy_attachment" "basic_execution" {
  role       = aws_iam_role.scan.name
  policy_arn = "arn:${data.aws_partition.current.partition}:iam::aws:policy/service-role/AWSLambdaBasicExecutionRole"
}

data "aws_iam_policy_document" "scan" {
  statement {
    sid       = "ConsumeScanQueue"
    actions   = ["sqs:ReceiveMessage", "sqs:DeleteMessage", "sqs:GetQueueAttributes"]
    resources = [aws_sqs_queue.scan.arn]
  }

  statement {
    sid       = "ReadAndRemoveFromQuarantine"
    actions   = ["s3:GetObject", "s3:DeleteObject"]
    resources = ["${var.quarantine_bucket_arn}/*"]
  }

  # Etap 1: skaner sam przenosi plik. W etapie 2 robią to osobne role
  # finalize-clean i handle-infected uruchamiane przez Step Functions.
  statement {
    sid       = "WriteScanResult"
    actions   = ["s3:PutObject"]
    resources = ["${var.clean_bucket_arn}/*", "${var.infected_bucket_arn}/*"]
  }

  statement {
    sid       = "TransitionAssetStatus"
    actions   = ["dynamodb:GetItem", "dynamodb:UpdateItem"]
    resources = [var.assets_table_arn]
  }

  statement {
    sid       = "SecurityAlert"
    actions   = ["sns:Publish"]
    resources = [aws_sns_topic.alerts.arn]
  }
}

resource "aws_iam_policy" "scan" {
  name        = "dam-scan-main"
  description = "Uprawnienia funkcji scan"
  policy      = data.aws_iam_policy_document.scan.json
}

resource "aws_iam_role_policy_attachment" "scan" {
  role       = aws_iam_role.scan.name
  policy_arn = aws_iam_policy.scan.arn
}

resource "aws_cloudwatch_log_group" "scan" {
  name              = "/aws/lambda/${local.name}"
  retention_in_days = var.log_retention_days
}

resource "aws_lambda_function" "scan" {
  #checkov:skip=CKV_AWS_116:Wywoływana przez SQS z własną kolejką DLQ (redrive)
  #checkov:skip=CKV_AWS_115:Współbieżność ogranicza maximum_concurrency wyzwalacza SQS; reserved concurrency niedostępne przy limicie konta 10
  count = local.create_lambda ? 1 : 0

  function_name = local.name
  description   = "Skan antywirusowy ClamAV plików z kwarantanny"
  role          = aws_iam_role.scan.arn
  package_type  = "Image"
  image_uri     = var.image_uri
  # x86_64: obraz buduje się natywnie na runnerach GitHub (ADR 0014).
  architectures = ["x86_64"]

  # ClamAV trzyma bazę sygnatur w pamięci (~1,2 GB); plik do 1 GB w /tmp.
  memory_size = 3008
  timeout     = local.scan_timeout_s

  ephemeral_storage {
    size = 2048
  }

  logging_config {
    log_format            = "JSON"
    log_group             = aws_cloudwatch_log_group.scan.name
    application_log_level = "INFO"
    system_log_level      = "WARN"
  }

  environment {
    variables = {
      RUST_LOG          = "info"
      ASSETS_TABLE      = var.assets_table_name
      QUARANTINE_BUCKET = var.quarantine_bucket
      CLEAN_BUCKET      = var.clean_bucket
      INFECTED_BUCKET   = var.infected_bucket
      ALERTS_TOPIC_ARN  = aws_sns_topic.alerts.arn
    }
  }

  depends_on = [
    aws_iam_role_policy_attachment.basic_execution,
    aws_iam_role_policy_attachment.scan,
    aws_cloudwatch_log_group.scan,
  ]
}

resource "aws_lambda_event_source_mapping" "scan" {
  count = local.create_lambda ? 1 : 0

  event_source_arn        = aws_sqs_queue.scan.arn
  function_name           = aws_lambda_function.scan[0].arn
  batch_size              = 1
  function_response_types = ["ReportBatchItemFailures"]

  # Najwyżej 2 skany naraz: kontrola kosztów (rozdział 13).
  scaling_config {
    maximum_concurrency = 2
  }
}
