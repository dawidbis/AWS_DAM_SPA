# Buckety na pliki od użytkowników (rozdział 9):
#   quarantine – pliki prosto od użytkowników, czytane wyłącznie przez pipeline,
#   clean      – zweryfikowane pliki, udostępniane przez presigned URL,
#   infected   – zainfekowane pliki jako dowód incydentu.
# Polityki bucketów to druga linia obrony obok polityk IAM ról (rozdział 10.2).

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
  role_arn_prefix = "arn:${data.aws_partition.current.partition}:iam::${data.aws_caller_identity.current.account_id}:role"

  buckets = {
    for name, access in var.bucket_access : name => {
      readers = [for role in access.readers : "${local.role_arn_prefix}/${role}"]
      writers = [for role in access.writers : "${local.role_arn_prefix}/${role}"]
    }
  }

  lifecycle = {
    quarantine = { expiration_days = var.quarantine_retention_days, noncurrent_days = 1 }
    clean      = { expiration_days = null, noncurrent_days = 30 }
    infected   = { expiration_days = var.infected_retention_days, noncurrent_days = var.infected_retention_days }
  }
}

resource "aws_s3_bucket" "this" {
  for_each = local.buckets

  bucket        = "${var.name_prefix}-${each.key}-${data.aws_caller_identity.current.account_id}"
  force_destroy = var.force_destroy
}

resource "aws_s3_bucket_ownership_controls" "this" {
  for_each = aws_s3_bucket.this

  bucket = each.value.id

  rule {
    object_ownership = "BucketOwnerEnforced"
  }
}

resource "aws_s3_bucket_public_access_block" "this" {
  for_each = aws_s3_bucket.this

  bucket = each.value.id

  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

resource "aws_s3_bucket_server_side_encryption_configuration" "this" {
  for_each = aws_s3_bucket.this

  bucket = each.value.id

  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm = "AES256"
    }
  }
}

resource "aws_s3_bucket_versioning" "this" {
  for_each = aws_s3_bucket.this

  bucket = each.value.id

  versioning_configuration {
    status = "Enabled"
  }
}

resource "aws_s3_bucket_logging" "this" {
  for_each = aws_s3_bucket.this

  bucket        = each.value.id
  target_bucket = var.log_bucket_id
  target_prefix = "s3/${each.value.id}/"
}

# Kwarantanna: plik czeka tylko na skan, potem znika (kopia trafia do clean
# albo infected). Dowody incydentów trzymamy dłużej (Object Lock w etapie 2).
# Wszędzie przerywamy porzucone uploady multipart.
resource "aws_s3_bucket_lifecycle_configuration" "this" {
  for_each = aws_s3_bucket.this

  bucket = each.value.id

  rule {
    id     = "retention"
    status = "Enabled"

    filter {}

    dynamic "expiration" {
      for_each = local.lifecycle[each.key].expiration_days == null ? [] : [1]

      content {
        days = local.lifecycle[each.key].expiration_days
      }
    }

    noncurrent_version_expiration {
      noncurrent_days = local.lifecycle[each.key].noncurrent_days
    }

    abort_incomplete_multipart_upload {
      days_after_initiation = 2
    }
  }

  depends_on = [aws_s3_bucket_versioning.this]
}

# Przeglądarka wysyła części pliku bezpośrednio do kwarantanny (presigned PUT)
# i musi odczytać ETag każdej części, żeby zakończyć upload multipart.
resource "aws_s3_bucket_cors_configuration" "quarantine" {
  bucket = aws_s3_bucket.this["quarantine"].id

  cors_rule {
    allowed_methods = ["PUT"]
    allowed_origins = var.upload_allowed_origins
    allowed_headers = ["content-type", "content-md5", "x-amz-checksum-crc32", "x-amz-checksum-sha256", "x-amz-sdk-checksum-algorithm"]
    expose_headers  = ["ETag"]
    max_age_seconds = 3000
  }
}

data "aws_iam_policy_document" "bucket" {
  for_each = local.buckets

  statement {
    sid     = "DenyInsecureTransport"
    effect  = "Deny"
    actions = ["s3:*"]
    resources = [
      aws_s3_bucket.this[each.key].arn,
      "${aws_s3_bucket.this[each.key].arn}/*",
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

  # Odczyt i zapis obiektów tylko przez wskazane role (rozdział 10.2). Presigned
  # URL działa z uprawnieniami roli, która go podpisała, więc np. przeglądarka
  # wgrywa do kwarantanny jako dam-upload-init. Pusta lista = nikt.
  dynamic "statement" {
    for_each = {
      DenyReadExceptAllowedRoles  = { actions = ["s3:GetObject", "s3:GetObjectVersion"], roles = each.value.readers }
      DenyWriteExceptAllowedRoles = { actions = ["s3:PutObject"], roles = each.value.writers }
    }

    content {
      sid       = statement.key
      effect    = "Deny"
      actions   = statement.value.actions
      resources = ["${aws_s3_bucket.this[each.key].arn}/*"]

      principals {
        type        = "*"
        identifiers = ["*"]
      }

      dynamic "condition" {
        for_each = length(statement.value.roles) > 0 ? [1] : []

        content {
          test     = "ArnNotLike"
          variable = "aws:PrincipalArn"
          values   = statement.value.roles
        }
      }
    }
  }
}

resource "aws_s3_bucket_policy" "this" {
  for_each = local.buckets

  bucket = aws_s3_bucket.this[each.key].id
  policy = data.aws_iam_policy_document.bucket[each.key].json

  depends_on = [aws_s3_bucket_public_access_block.this]
}
