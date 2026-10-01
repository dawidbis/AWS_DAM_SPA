locals {
  name_prefix = "${var.project}-${var.environment}"
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
