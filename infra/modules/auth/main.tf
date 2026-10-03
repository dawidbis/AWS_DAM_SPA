# Cognito User Pool z grupami A–D (rozdział 4) i klientem publicznym dla SPA.
# Logowanie przez managed login (OAuth 2.0 authorization code + PKCE).
# Samodzielna rejestracja jest wyłączona: konta zakłada administrator.

terraform {
  required_version = ">= 1.10"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = ">= 6.0"
    }
  }
}

resource "aws_cognito_user_pool" "this" {
  name = var.name

  # Essentials: managed login w nowej wersji, 10 000 MAU w Free Tier.
  user_pool_tier      = "ESSENTIALS"
  deletion_protection = var.deletion_protection ? "ACTIVE" : "INACTIVE"

  username_attributes      = ["email"]
  auto_verified_attributes = ["email"]

  username_configuration {
    case_sensitive = false
  }

  admin_create_user_config {
    allow_admin_create_user_only = true

    invite_message_template {
      email_subject = "Matchday DAM: zaproszenie"
      email_message = "Konto w Matchday DAM zostało utworzone. Login: {username}, hasło tymczasowe: {####}. Przy pierwszym logowaniu ustawisz własne hasło."
      sms_message   = "Matchday DAM: login {username}, hasło tymczasowe {####}"
    }
  }

  password_policy {
    minimum_length                   = 12
    require_lowercase                = true
    require_uppercase                = true
    require_numbers                  = true
    require_symbols                  = false
    temporary_password_validity_days = 7
  }

  mfa_configuration = "OPTIONAL"

  software_token_mfa_configuration {
    enabled = true
  }

  account_recovery_setting {
    recovery_mechanism {
      name     = "verified_email"
      priority = 1
    }
  }

  # Domyślna wysyłka Cognito (limit ~50 maili dziennie) wystarcza dla kont testowych.
  email_configuration {
    email_sending_account = "COGNITO_DEFAULT"
  }
}

resource "aws_cognito_user_group" "this" {
  for_each = var.groups

  user_pool_id = aws_cognito_user_pool.this.id
  name         = each.key
  description  = each.value.description
  precedence   = each.value.precedence
}

resource "aws_cognito_user_pool_domain" "this" {
  domain                = var.domain_prefix
  user_pool_id          = aws_cognito_user_pool.this.id
  managed_login_version = 2
}

resource "aws_cognito_user_pool_client" "spa" {
  name         = "${var.name}-spa"
  user_pool_id = aws_cognito_user_pool.this.id

  # Klient publiczny (przeglądarka): bez sekretu, ochronę daje PKCE.
  generate_secret = false

  allowed_oauth_flows_user_pool_client = true
  allowed_oauth_flows                  = ["code"]
  allowed_oauth_scopes                 = ["openid", "email", "profile"]
  supported_identity_providers         = ["COGNITO"]
  callback_urls                        = var.callback_urls
  logout_urls                          = var.logout_urls

  # Bez ALLOW_USER_PASSWORD_AUTH: hasło nigdy nie trafia do API w jawnej
  # postaci, a SPA loguje się wyłącznie przez managed login.
  explicit_auth_flows = ["ALLOW_USER_SRP_AUTH", "ALLOW_REFRESH_TOKEN_AUTH"]

  access_token_validity  = 60
  id_token_validity      = 60
  refresh_token_validity = 12

  token_validity_units {
    access_token  = "minutes"
    id_token      = "minutes"
    refresh_token = "hours"
  }

  prevent_user_existence_errors = "ENABLED"
  enable_token_revocation       = true
}

resource "aws_cognito_managed_login_branding" "spa" {
  user_pool_id = aws_cognito_user_pool.this.id
  client_id    = aws_cognito_user_pool_client.spa.id

  use_cognito_provided_values = true
}

# Klient testów e2e (tests/e2e): logowanie administracyjne (AdminInitiateAuth),
# które wymaga poświadczeń AWS z uprawnieniem cognito-idp:AdminInitiateAuth.
# Przeglądarka nie może go użyć, a SPA nadal loguje się tylko przez managed login.
resource "aws_cognito_user_pool_client" "e2e" {
  count = var.create_e2e_client ? 1 : 0

  name            = "${var.name}-e2e"
  user_pool_id    = aws_cognito_user_pool.this.id
  generate_secret = false

  explicit_auth_flows = ["ALLOW_ADMIN_USER_PASSWORD_AUTH", "ALLOW_REFRESH_TOKEN_AUTH"]

  access_token_validity  = 15
  id_token_validity      = 15
  refresh_token_validity = 1

  token_validity_units {
    access_token  = "minutes"
    id_token      = "minutes"
    refresh_token = "hours"
  }

  prevent_user_existence_errors = "ENABLED"
  enable_token_revocation       = true
}
