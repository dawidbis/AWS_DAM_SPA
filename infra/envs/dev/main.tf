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

module "api" {
  source = "../../modules/http-api"

  name            = local.name_prefix
  allowed_origins = local.spa_origins
  jwt_issuer      = module.auth.issuer_url
  jwt_audience    = [module.auth.client_id]

  routes = {
    "GET /me" = { function_name = module.api_me.function_name, function_arn = module.api_me.function_arn }
  }
}
