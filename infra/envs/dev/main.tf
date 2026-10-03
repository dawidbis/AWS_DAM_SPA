locals {
  name_prefix = "${var.project}-${var.environment}"

  # config.json dla Angulara. Adres powrotu po logowaniu frontend wylicza
  # z window.location.origin, więc config zawiera tylko dane Cognito.
  frontend_config = {
    region     = var.region
    authority  = module.auth.issuer_url
    authDomain = module.auth.domain_url
    clientId   = module.auth.client_id
    apiUrl     = module.api.url
  }

  # Originy SPA: wdrożony frontend i lokalny `ng serve`.
  spa_origins = [module.frontend.url, "http://localhost:4200"]
}

# Boundary tworzone w infra/bootstrap. Rola deployu może zakładać role
# wyłącznie z tym boundary, więc każda rola w tym środowisku musi je mieć.
data "aws_iam_policy" "permissions_boundary" {
  name = "dam-permissions-boundary"
}

module "hello_world" {
  source = "../../modules/rust-lambda"

  name                     = "hello-world"
  function_name            = "${local.name_prefix}-hello-world"
  description              = "Etap 0: weryfikacja łańcucha build -> deploy dla Lambd w Ruście"
  zip_path                 = "${var.lambda_artifacts_dir}/hello-world/bootstrap.zip"
  permissions_boundary_arn = data.aws_iam_policy.permissions_boundary.arn
}

data "aws_caller_identity" "current" {}

# --- Logi dostępu (S3, CloudFront) -----------------------------------------

module "access_logs" {
  source = "../../modules/access-logs"

  bucket_name = "${local.name_prefix}-access-logs-${data.aws_caller_identity.current.account_id}"
}

# --- Frontend (S3 + CloudFront) ---------------------------------------------

module "frontend" {
  source = "../../modules/frontend-hosting"

  name        = local.name_prefix
  bucket_name = "${local.name_prefix}-frontend-${data.aws_caller_identity.current.account_id}"

  runtime_config         = local.frontend_config
  log_bucket_id          = module.access_logs.bucket_id
  log_bucket_domain_name = module.access_logs.bucket_domain_name
}

# --- Uwierzytelnianie (Cognito) ---------------------------------------------

module "auth" {
  source = "../../modules/auth"

  name          = local.name_prefix
  domain_prefix = "${local.name_prefix}-${data.aws_caller_identity.current.account_id}"

  callback_urls = [
    "${module.frontend.url}/auth/callback",
    "http://localhost:4200/auth/callback",
  ]
  logout_urls = [
    "${module.frontend.url}/",
    "http://localhost:4200/",
  ]

  # Grupy A–D z rozdziału 4 dokumentu projektu.
  groups = {
    admin       = { description = "A: media manager, dział komunikacji", precedence = 1 }
    staff       = { description = "B: marketing, redakcja, social media", precedence = 2 }
    contributor = { description = "C: fotografowie meczowi, agencje", precedence = 3 }
    viewer      = { description = "D: sponsorzy, partnerzy, media", precedence = 4 }
  }
}

# --- Pliki (kwarantanna, clean, infected) ------------------------------------

module "storage" {
  source = "../../modules/storage"

  name_prefix            = local.name_prefix
  upload_allowed_origins = local.spa_origins
  log_bucket_id          = module.access_logs.bucket_id
}

# --- API ---------------------------------------------------------------------

module "api_me" {
  source = "../../modules/rust-lambda"

  name                     = "api-me"
  function_name            = "${local.name_prefix}-api-me"
  description              = "GET /me: tożsamość i grupy wywołującego"
  zip_path                 = "${var.lambda_artifacts_dir}/api-me/bootstrap.zip"
  permissions_boundary_arn = data.aws_iam_policy.permissions_boundary.arn
}

# --- Dane ----------------------------------------------------------------------

module "data" {
  source = "../../modules/data"

  name_prefix = local.name_prefix
}

# --- Skanowanie (EventBridge → SQS → Step Functions scan-pipeline) ------------------

module "scanner" {
  source = "../../modules/scanner"

  name_prefix              = local.name_prefix
  image_uri                = var.scanner_image_uri
  permissions_boundary_arn = data.aws_iam_policy.permissions_boundary.arn
  alert_email              = var.alert_email

  lambda_artifacts_dir = var.lambda_artifacts_dir
  assets_table_name    = module.data.assets_table_name
  assets_table_arn     = module.data.assets_table_arn
  incidents_table_name = module.data.incidents_table_name
  incidents_table_arn  = module.data.incidents_table_arn

  quarantine_bucket     = module.storage.bucket_names["quarantine"]
  quarantine_bucket_arn = module.storage.bucket_arns["quarantine"]
  clean_bucket          = module.storage.bucket_names["clean"]
  clean_bucket_arn      = module.storage.bucket_arns["clean"]
  infected_bucket       = module.storage.bucket_names["infected"]
  infected_bucket_arn   = module.storage.bucket_arns["infected"]
}

# --- Upload multipart -----------------------------------------------------------
# Każda funkcja ma własną rolę i wyłącznie potrzebne uprawnienia (rozdział 10.1).
# Polityka bucketu kwarantanny dopuszcza zapis tylko tych trzech ról.

locals {
  quarantine_arn = module.storage.bucket_arns["quarantine"]

  upload_lambdas = {
    upload-init     = "POST /uploads: zakłada upload multipart i wystawia presigned URL-e"
    upload-status   = "GET /uploads/{assetId}: stan uploadu i URL-e do wznowienia"
    upload-complete = "POST /uploads/{assetId}/complete: weryfikuje rozmiar i kończy upload"
  }
}

data "aws_iam_policy_document" "upload_init" {
  statement {
    sid       = "CreateMultipartUploadAndPresignParts"
    actions   = ["s3:PutObject"]
    resources = ["${local.quarantine_arn}/*"]
  }

  statement {
    sid       = "CreateAsset"
    actions   = ["dynamodb:PutItem"]
    resources = [module.data.assets_table_arn]
  }
}

data "aws_iam_policy_document" "upload_status" {
  statement {
    sid       = "ListAndPresignParts"
    actions   = ["s3:ListMultipartUploadParts", "s3:PutObject"]
    resources = ["${local.quarantine_arn}/*"]
  }

  statement {
    sid       = "ReadAsset"
    actions   = ["dynamodb:GetItem"]
    resources = [module.data.assets_table_arn]
  }
}

data "aws_iam_policy_document" "upload_complete" {
  statement {
    sid       = "CompleteOrAbortUpload"
    actions   = ["s3:ListMultipartUploadParts", "s3:PutObject", "s3:AbortMultipartUpload"]
    resources = ["${local.quarantine_arn}/*"]
  }

  statement {
    sid       = "ReadAndTransitionAsset"
    actions   = ["dynamodb:GetItem", "dynamodb:UpdateItem"]
    resources = [module.data.assets_table_arn]
  }
}

module "upload_lambdas" {
  source   = "../../modules/rust-lambda"
  for_each = local.upload_lambdas

  name                     = each.key
  function_name            = "${local.name_prefix}-${each.key}"
  description              = each.value
  zip_path                 = "${var.lambda_artifacts_dir}/api-${each.key}/bootstrap.zip"
  permissions_boundary_arn = data.aws_iam_policy.permissions_boundary.arn
  memory_size              = 256

  policies = {
    main = {
      upload-init     = data.aws_iam_policy_document.upload_init.json
      upload-status   = data.aws_iam_policy_document.upload_status.json
      upload-complete = data.aws_iam_policy_document.upload_complete.json
    }[each.key]
  }

  environment = {
    ASSETS_TABLE      = module.data.assets_table_name
    QUARANTINE_BUCKET = module.storage.bucket_names["quarantine"]
  }
}

# --- Katalog: galeria, moje zgłoszenia, publikacja, pobieranie -------------------
# dam-assets-read jest jedyną rolą z odczytem bucketu clean (polityka bucketu),
# więc tylko ta funkcja podpisuje linki do plików.

data "aws_iam_policy_document" "assets_read" {
  statement {
    sid     = "ReadAssets"
    actions = ["dynamodb:GetItem", "dynamodb:Query"]
    resources = [
      module.data.assets_table_arn,
      "${module.data.assets_table_arn}/index/status-index",
      "${module.data.assets_table_arn}/index/uploader-index",
    ]
  }

  statement {
    sid       = "PresignCleanObjects"
    actions   = ["s3:GetObject"]
    resources = ["${module.storage.bucket_arns["clean"]}/*"]
  }
}

data "aws_iam_policy_document" "asset_publish" {
  statement {
    sid       = "TransitionAssetStatus"
    actions   = ["dynamodb:UpdateItem"]
    resources = [module.data.assets_table_arn]
  }
}

module "assets_read" {
  source = "../../modules/rust-lambda"

  name                     = "assets-read"
  function_name            = "${local.name_prefix}-assets-read"
  description              = "GET /assets i GET /assets/{assetId}/download: katalog i presigned URL-e do plików"
  zip_path                 = "${var.lambda_artifacts_dir}/api-assets-read/bootstrap.zip"
  permissions_boundary_arn = data.aws_iam_policy.permissions_boundary.arn
  memory_size              = 256

  policies = { main = data.aws_iam_policy_document.assets_read.json }

  environment = {
    ASSETS_TABLE = module.data.assets_table_name
    CLEAN_BUCKET = module.storage.bucket_names["clean"]
  }
}

module "asset_publish" {
  source = "../../modules/rust-lambda"

  name                     = "asset-publish"
  function_name            = "${local.name_prefix}-asset-publish"
  description              = "POST /assets/{assetId}/publish: publikacja przez A (warunkowa zmiana statusu)"
  zip_path                 = "${var.lambda_artifacts_dir}/api-asset-publish/bootstrap.zip"
  permissions_boundary_arn = data.aws_iam_policy.permissions_boundary.arn

  policies = { main = data.aws_iam_policy_document.asset_publish.json }

  environment = {
    ASSETS_TABLE = module.data.assets_table_name
  }
}

# Ponowienie skanu po SCAN_FAILED (A).
data "aws_iam_policy_document" "asset_rescan" {
  statement {
    sid       = "TransitionAssetStatus"
    actions   = ["dynamodb:UpdateItem"]
    resources = [module.data.assets_table_arn]
  }

  statement {
    sid       = "StartScanPipeline"
    actions   = ["states:StartExecution"]
    resources = [module.scanner.state_machine_arn]
  }
}

module "asset_rescan" {
  source = "../../modules/rust-lambda"

  name                     = "asset-rescan"
  function_name            = "${local.name_prefix}-asset-rescan"
  description              = "POST /assets/{assetId}/rescan: ponowienie skanu przez A po SCAN_FAILED"
  zip_path                 = "${var.lambda_artifacts_dir}/api-asset-rescan/bootstrap.zip"
  permissions_boundary_arn = data.aws_iam_policy.permissions_boundary.arn

  policies = { main = data.aws_iam_policy_document.asset_rescan.json }

  environment = {
    ASSETS_TABLE      = module.data.assets_table_name
    STATE_MACHINE_ARN = module.scanner.state_machine_arn
  }
}

module "api" {
  source = "../../modules/http-api"

  name            = local.name_prefix
  allowed_origins = local.spa_origins
  jwt_issuer      = module.auth.issuer_url
  jwt_audience    = [module.auth.client_id]

  routes = {
    "GET /me"                          = { function_name = module.api_me.function_name, function_arn = module.api_me.function_arn }
    "POST /uploads"                    = { function_name = module.upload_lambdas["upload-init"].function_name, function_arn = module.upload_lambdas["upload-init"].function_arn }
    "GET /uploads/{assetId}"           = { function_name = module.upload_lambdas["upload-status"].function_name, function_arn = module.upload_lambdas["upload-status"].function_arn }
    "POST /uploads/{assetId}/complete" = { function_name = module.upload_lambdas["upload-complete"].function_name, function_arn = module.upload_lambdas["upload-complete"].function_arn }
    "GET /assets"                      = { function_name = module.assets_read.function_name, function_arn = module.assets_read.function_arn }
    "GET /assets/{assetId}/download"   = { function_name = module.assets_read.function_name, function_arn = module.assets_read.function_arn }
    "POST /assets/{assetId}/publish"   = { function_name = module.asset_publish.function_name, function_arn = module.asset_publish.function_arn }
    "POST /assets/{assetId}/rescan"    = { function_name = module.asset_rescan.function_name, function_arn = module.asset_rescan.function_arn }
  }
}
