terraform {
  # use_lockfile (natywne blokowanie stanu w S3) wymaga Terraform >= 1.10.
  required_version = ">= 1.10"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 6.0"
    }
  }

  # Pierwsze `apply` wykonujemy z lokalnym stanem (bucket jeszcze nie istnieje).
  # Następnie stan bootstrapu przenosimy do utworzonego bucketu, patrz
  # docs/setup-aws.md, krok 4.
  # backend "s3" {
  #   key          = "bootstrap/terraform.tfstate"
  #   region       = "eu-central-1"
  #   use_lockfile = true
  #   encrypt      = true
  # }
}

provider "aws" {
  region = var.region

  default_tags {
    tags = {
      Project   = var.project
      Stack     = "bootstrap"
      ManagedBy = "terraform"
    }
  }
}
