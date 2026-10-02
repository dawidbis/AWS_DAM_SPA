variable "region" {
  type    = string
  default = "eu-central-1"
}

variable "project" {
  type    = string
  default = "matchday-dam"
}

variable "environment" {
  type    = string
  default = "dev"
}

variable "lambda_artifacts_dir" {
  description = "Katalog z artefaktami Cargo Lambda (<dir>/<crate>/bootstrap.zip)."
  type        = string
  default     = "../../../lambdas/target/lambda"
}

variable "scanner_image_uri" {
  description = "Obraz skanera ClamAV w ECR (repo@sha256:...). Ustawia CI; pusty = Lambda scan jeszcze nie powstaje."
  type        = string
  default     = ""
}

variable "alert_email" {
  description = "Adres alertów bezpieczeństwa (zmienna ALERT_EMAIL w GitHub Actions)."
  type        = string
  default     = ""
}
