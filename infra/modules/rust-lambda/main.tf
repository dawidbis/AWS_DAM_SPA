# Funkcja Lambda w Ruście (Cargo Lambda, provided.al2023, arm64) z własną,
# dedykowaną rolą IAM. Każda funkcja projektu dostaje osobną rolę (rozdział 7.1).

terraform {
  required_version = ">= 1.10"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = ">= 6.0"
    }
  }
}

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

data "aws_caller_identity" "current" {}

data "aws_partition" "current" {}

resource "aws_iam_role" "this" {
  name                 = "dam-${var.name}"
  description          = "Rola funkcji Lambda ${var.name}"
  assume_role_policy   = data.aws_iam_policy_document.trust.json
  permissions_boundary = var.permissions_boundary_arn
}

resource "aws_iam_role_policy_attachment" "basic_execution" {
  role       = aws_iam_role.this.name
  policy_arn = "arn:${data.aws_partition.current.partition}:iam::aws:policy/service-role/AWSLambdaBasicExecutionRole"
}

resource "aws_iam_role_policy_attachment" "extra" {
  for_each = var.policy_arns

  role       = aws_iam_role.this.name
  policy_arn = each.value
}

resource "aws_cloudwatch_log_group" "this" {
  name              = "/aws/lambda/${var.function_name}"
  retention_in_days = var.log_retention_days
}

resource "aws_lambda_function" "this" {
  function_name = var.function_name
  description   = var.description
  role          = aws_iam_role.this.arn

  filename         = var.zip_path
  source_code_hash = filebase64sha256(var.zip_path)
  runtime          = "provided.al2023"
  handler          = "bootstrap"
  architectures    = ["arm64"]

  memory_size                    = var.memory_size
  timeout                        = var.timeout
  reserved_concurrent_executions = var.reserved_concurrency

  logging_config {
    log_format            = "JSON"
    log_group             = aws_cloudwatch_log_group.this.name
    application_log_level = var.log_level
    system_log_level      = "WARN"
  }

  environment {
    variables = merge({ RUST_LOG = lower(var.log_level) }, var.environment)
  }

  depends_on = [
    aws_iam_role_policy_attachment.basic_execution,
    aws_cloudwatch_log_group.this,
  ]
}
