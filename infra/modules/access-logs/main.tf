# Wspólny bucket na logi dostępu: logi serwerowe S3 i standardowe logi
# CloudFront. Przydają się przy analizie incydentów (kto pobierał co i kiedy).
# Retencja krótka, żeby logi nie generowały kosztów (rozdział 13).

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

data "aws_canonical_user_id" "current" {}

# Konto AWS, z którego CloudFront zapisuje standardowe logi (stała AWS).
locals {
  cloudfront_log_delivery_canonical_id = "c4c1ede66af53448b93c283ce9448c4ba468c9432aa01d700d3878632f77d2d0"
}

# Bucket na logi nie loguje sam do siebie (pętla zapisu logów o logach).
resource "aws_s3_bucket" "logs" { # NOSONAR
  #checkov:skip=CKV2_AWS_62:Powiadomienia o nowych logach nie są potrzebne
  bucket        = var.bucket_name
  force_destroy = true
}

# Standardowe logi CloudFront wymagają włączonych ACL na buckecie docelowym.
resource "aws_s3_bucket_ownership_controls" "logs" {
  #checkov:skip=CKV2_AWS_65:CloudFront standard logging zapisuje przez ACL
  bucket = aws_s3_bucket.logs.id

  rule {
    object_ownership = "BucketOwnerPreferred"
  }
}

resource "aws_s3_bucket_acl" "logs" {
  bucket = aws_s3_bucket.logs.id

  access_control_policy {
    owner {
      id = data.aws_canonical_user_id.current.id
    }

    grant {
      permission = "FULL_CONTROL"

      grantee {
        type = "CanonicalUser"
        id   = data.aws_canonical_user_id.current.id
      }
    }

    grant {
      permission = "FULL_CONTROL"

      grantee {
        type = "CanonicalUser"
        id   = local.cloudfront_log_delivery_canonical_id
      }
    }
  }

  depends_on = [aws_s3_bucket_ownership_controls.logs]
}

resource "aws_s3_bucket_public_access_block" "logs" {
  bucket = aws_s3_bucket.logs.id

  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

resource "aws_s3_bucket_server_side_encryption_configuration" "logs" {
  bucket = aws_s3_bucket.logs.id

  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm = "AES256"
    }
  }
}

resource "aws_s3_bucket_versioning" "logs" {
  bucket = aws_s3_bucket.logs.id

  versioning_configuration {
    status = "Enabled"
  }
}

resource "aws_s3_bucket_lifecycle_configuration" "logs" {
  bucket = aws_s3_bucket.logs.id

  rule {
    id     = "expire-logs"
    status = "Enabled"

    filter {}

    expiration {
      days = var.retention_days
    }

    noncurrent_version_expiration {
      noncurrent_days = 1
    }

    abort_incomplete_multipart_upload {
      days_after_initiation = 1
    }
  }

  depends_on = [aws_s3_bucket_versioning.logs]
}

data "aws_iam_policy_document" "logs" {
  statement {
    sid       = "AllowS3ServerAccessLogs"
    actions   = ["s3:PutObject"]
    resources = ["${aws_s3_bucket.logs.arn}/s3/*"]

    principals {
      type        = "Service"
      identifiers = ["logging.s3.amazonaws.com"]
    }

    condition {
      test     = "StringEquals"
      variable = "aws:SourceAccount"
      values   = [data.aws_caller_identity.current.account_id]
    }
  }

  statement {
    sid     = "DenyInsecureTransport"
    effect  = "Deny"
    actions = ["s3:*"]
    resources = [
      aws_s3_bucket.logs.arn,
      "${aws_s3_bucket.logs.arn}/*",
    ]

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

resource "aws_s3_bucket_policy" "logs" {
  bucket = aws_s3_bucket.logs.id
  policy = data.aws_iam_policy_document.logs.json

  depends_on = [aws_s3_bucket_public_access_block.logs]
}
