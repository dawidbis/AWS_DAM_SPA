output "state_bucket" {
  description = "Bucket na stan Terraform (-backend-config=\"bucket=...\")."
  value       = aws_s3_bucket.tfstate.bucket
}

output "permissions_boundary_arn" {
  description = "ARN permission boundary, które musi mieć każda rola projektu."
  value       = aws_iam_policy.permissions_boundary.arn
}

output "github_plan_role_arn" {
  value = aws_iam_role.github_plan.arn
}

output "github_deploy_role_arn" {
  value = aws_iam_role.github_deploy.arn
}

output "github_actions_variables" {
  description = "Zmienne do ustawienia w GitHub: Settings → Secrets and variables → Actions → Variables."
  value = {
    AWS_REGION          = var.region
    AWS_PLAN_ROLE_ARN   = aws_iam_role.github_plan.arn
    AWS_DEPLOY_ROLE_ARN = aws_iam_role.github_deploy.arn
    TF_STATE_BUCKET     = aws_s3_bucket.tfstate.bucket
  }
}
