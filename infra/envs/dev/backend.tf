terraform {
  # Nazwa bucketu zawiera ID konta, więc podajemy ją przy init:
  #   terraform init -backend-config="bucket=<TF_STATE_BUCKET>"
  # (robi to `just init` oraz workflowy GitHub Actions).
  backend "s3" {
    key          = "envs/dev/terraform.tfstate"
    region       = "eu-central-1"
    use_lockfile = true
    encrypt      = true
  }
}
