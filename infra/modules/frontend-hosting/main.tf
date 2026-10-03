# Hosting SPA: prywatny bucket S3 dostępny wyłącznie przez CloudFront
# (Origin Access Control), nagłówki bezpieczeństwa (CSP, HSTS, nosniff,
# X-Frame-Options, Permissions-Policy) i config.json dla Angulara.
# Pliki aplikacji wgrywa scripts/deploy-frontend.sh (krok w deploy.yml).

terraform {
  required_version = ">= 1.10"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = ">= 6.0"
    }
  }
}

resource "aws_s3_bucket" "site" {
  bucket        = var.bucket_name
  force_destroy = true # zawartość to artefakt buildu, odtwarzany z CI
}

resource "aws_s3_bucket_logging" "site" {
  bucket        = aws_s3_bucket.site.id
  target_bucket = var.log_bucket_id
  target_prefix = "s3/${var.bucket_name}/"
}

resource "aws_s3_bucket_ownership_controls" "site" {
  bucket = aws_s3_bucket.site.id

  rule {
    object_ownership = "BucketOwnerEnforced"
  }
}

resource "aws_s3_bucket_public_access_block" "site" {
  bucket = aws_s3_bucket.site.id

  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

resource "aws_s3_bucket_server_side_encryption_configuration" "site" {
  bucket = aws_s3_bucket.site.id

  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm = "AES256"
    }
  }
}

resource "aws_s3_bucket_versioning" "site" {
  bucket = aws_s3_bucket.site.id

  versioning_configuration {
    status = "Enabled"
  }
}

resource "aws_s3_bucket_lifecycle_configuration" "site" {
  bucket = aws_s3_bucket.site.id

  rule {
    id     = "expire-old-builds"
    status = "Enabled"

    filter {}

    noncurrent_version_expiration {
      noncurrent_days = 7
    }

    abort_incomplete_multipart_upload {
      days_after_initiation = 1
    }
  }

  depends_on = [aws_s3_bucket_versioning.site]
}

resource "aws_cloudfront_origin_access_control" "site" {
  name                              = var.bucket_name
  description                       = "Dostęp CloudFront do bucketu ${var.bucket_name}"
  origin_access_control_origin_type = "s3"
  signing_behavior                  = "always"
  signing_protocol                  = "sigv4"
}

data "aws_cloudfront_cache_policy" "optimized" {
  name = "Managed-CachingOptimized"
}

data "aws_region" "current" {}

locals {
  region = data.aws_region.current.region

  # Content Security Policy SPA (rozdział 7.2). Skrypty wyłącznie z własnej
  # domeny (Angular bez eval i bez skryptów inline: inlineCritical wyłączone
  # w angular.json). Style 'unsafe-inline', bo Angular wstrzykuje style
  # komponentów jako <style>; nonce wymagałby renderowania po stronie serwera.
  # Hosty API, Cognito i S3 jako wzorce regionu: dokładne adresy zależą od
  # zasobów, które same zależą od adresu CloudFront (cykl w Terraform).
  content_security_policy = join("; ", [
    "default-src 'self'",
    "script-src 'self'",
    "style-src 'self' 'unsafe-inline'",
    "img-src 'self' data: https://*.s3.${local.region}.amazonaws.com",
    "connect-src 'self' https://*.execute-api.${local.region}.amazonaws.com https://*.auth.${local.region}.amazoncognito.com https://cognito-idp.${local.region}.amazonaws.com https://*.s3.${local.region}.amazonaws.com",
    "worker-src 'self'",
    "font-src 'self'",
    "object-src 'none'",
    "base-uri 'self'",
    "form-action 'self'",
    "frame-ancestors 'none'",
    "upgrade-insecure-requests",
  ])
}

resource "aws_cloudfront_response_headers_policy" "security" {
  name    = "${var.name}-security-headers"
  comment = "Nagłówki bezpieczeństwa SPA: CSP, HSTS, nosniff, frame-ancestors"

  custom_headers_config {
    items {
      header   = "Permissions-Policy"
      value    = "camera=(), microphone=(), geolocation=(), payment=()"
      override = true
    }
  }

  security_headers_config {
    content_security_policy {
      content_security_policy = local.content_security_policy
      override                = true
    }

    strict_transport_security {
      access_control_max_age_sec = 31536000
      include_subdomains         = true
      preload                    = true
      override                   = true
    }

    content_type_options {
      override = true
    }

    frame_options {
      frame_option = "DENY"
      override     = true
    }

    referrer_policy {
      referrer_policy = "strict-origin-when-cross-origin"
      override        = true
    }
  }
}

resource "aws_cloudfront_distribution" "site" {
  #checkov:skip=CKV_AWS_68:WAF to stała opłata miesięczna; opcjonalny w rozdziale 2, rozważany w etapie 4 (publiczne demo)
  #checkov:skip=CKV2_AWS_47:Jak wyżej, brak WAF w dev
  #checkov:skip=CKV2_AWS_42:Domyślna domena *.cloudfront.net bez własnej domeny i certyfikatu ACM
  #checkov:skip=CKV_AWS_174:Przy domyślnym certyfikacie CloudFront nie da się ustawić minimum_protocol_version
  #checkov:skip=CKV_AWS_310:Origin failover wymaga drugiego bucketu w innym regionie (jeden region, rozdział 7.1)
  #checkov:skip=CKV_AWS_374:Brak ograniczeń geograficznych: sponsorzy i media z różnych krajów
  enabled             = true
  comment             = var.name
  default_root_object = "index.html"
  http_version        = "http2and3"
  is_ipv6_enabled     = true
  price_class         = "PriceClass_100"

  origin {
    origin_id                = "s3-site"
    domain_name              = aws_s3_bucket.site.bucket_regional_domain_name
    origin_access_control_id = aws_cloudfront_origin_access_control.site.id
  }

  default_cache_behavior {
    target_origin_id           = "s3-site"
    viewer_protocol_policy     = "redirect-to-https"
    allowed_methods            = ["GET", "HEAD"]
    cached_methods             = ["GET", "HEAD"]
    compress                   = true
    cache_policy_id            = data.aws_cloudfront_cache_policy.optimized.id
    response_headers_policy_id = aws_cloudfront_response_headers_policy.security.id
  }

  # Routing SPA: nieistniejąca ścieżka (np. /auth/callback) zwraca index.html,
  # a trasę obsługuje router Angulara. S3 z OAC zwraca 403 dla brakującego klucza.
  dynamic "custom_error_response" {
    for_each = [403, 404]

    content {
      error_code            = custom_error_response.value
      response_code         = 200
      response_page_path    = "/index.html"
      error_caching_min_ttl = 0
    }
  }

  logging_config {
    bucket          = var.log_bucket_domain_name
    prefix          = "cloudfront/${var.name}/"
    include_cookies = false
  }

  restrictions {
    geo_restriction {
      restriction_type = "none"
    }
  }

  viewer_certificate {
    cloudfront_default_certificate = true
  }
}

data "aws_iam_policy_document" "site_bucket" {
  statement {
    sid       = "AllowCloudFrontRead"
    actions   = ["s3:GetObject"]
    resources = ["${aws_s3_bucket.site.arn}/*"]

    principals {
      type        = "Service"
      identifiers = ["cloudfront.amazonaws.com"]
    }

    condition {
      test     = "StringEquals"
      variable = "AWS:SourceArn"
      values   = [aws_cloudfront_distribution.site.arn]
    }
  }

  statement {
    sid     = "DenyInsecureTransport"
    effect  = "Deny"
    actions = ["s3:*"]
    resources = [
      aws_s3_bucket.site.arn,
      "${aws_s3_bucket.site.arn}/*",
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

resource "aws_s3_bucket_policy" "site" {
  bucket = aws_s3_bucket.site.id
  policy = data.aws_iam_policy_document.site_bucket.json

  depends_on = [aws_s3_bucket_public_access_block.site]
}

# Konfiguracja runtime dla Angulara (adresy Cognito itp.). Jeden build
# frontendu działa w każdym środowisku, bo config czytany jest przy starcie.
resource "aws_s3_object" "config" {
  bucket        = aws_s3_bucket.site.id
  key           = "config.json"
  content       = jsonencode(var.runtime_config)
  content_type  = "application/json"
  cache_control = "no-cache"
  etag          = md5(jsonencode(var.runtime_config))
}
