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
