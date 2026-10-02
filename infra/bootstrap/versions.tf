terraform {
  # use_lockfile (natywne blokowanie stanu w S3) wymaga Terraform >= 1.10.
  required_version = ">= 1.10"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 6.0"
    }
  }

  # Stan bootstrapu leży w buckecie, który ten stos tworzy. Na nowym koncie
  # pierwsze `apply` robimy ze stanem lokalnym (backend_override.tf), potem
  # migrujemy go tutaj. Patrz docs/setup-aws.md, kroki 3-4.
  # Bucket: terraform init -backend-config="bucket=<state_bucket>"
  backend "s3" {
    key          = "bootstrap/terraform.tfstate"
    region       = "eu-central-1"
    use_lockfile = true
    encrypt      = true
  }
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
