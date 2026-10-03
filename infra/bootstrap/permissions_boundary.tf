# Permission boundary dla wszystkich ról tworzonych w projekcie: ról GitHub
# i ról Lambd/Step Functions zakładanych przez deploy. Boundary nie nadaje
# uprawnień, tylko wyznacza ich maksymalny zakres. Efektywne uprawnienia roli
# to część wspólna jej polityk i boundary.
#
# Najważniejsze gwarancje:
#   1. Tylko usługi używane w projekcie, tylko region projektu (+ us-east-1
#      dla usług globalnych: IAM, CloudFront, WAF dla CloudFront, Budgets).
#   2. Rola może tworzyć inne role wyłącznie z tym samym boundary, więc nie da
#      się „uciec" przez utworzenie roli bez ograniczeń.
#   3. Nikt z boundary nie zmieni samego boundary ani ról dam-github-*.
#   4. Bucket ze stanem Terraform jest chroniony przed usunięciem i zmianą polityki.

locals {
  permissions_boundary_name = "dam-permissions-boundary"
  permissions_boundary_arn  = "${local.iam_prefix}:policy/${local.permissions_boundary_name}"
}

data "aws_iam_policy_document" "permissions_boundary" {
  # Boundary z definicji obejmuje szeroki zakres ("Resource": "*") i jest
  # zawężane przez instrukcje Deny poniżej oraz polityki tożsamościowe ról.
  #checkov:skip=CKV_AWS_109:Boundary wyznacza górny limit uprawnień, nie nadaje ich
  #checkov:skip=CKV_AWS_111:Boundary wyznacza górny limit uprawnień, nie nadaje ich
  #checkov:skip=CKV_AWS_356:Boundary wyznacza górny limit uprawnień, nie nadaje ich
  statement {
    sid = "AllowProjectServices"
    actions = [
      "apigateway:*",
      "budgets:*",
      "ce:Get*",
      "cloudfront:*",
      "cloudwatch:*",
      "cognito-idp:*",
      "dynamodb:*",
      "ecr:*",
      "events:*",
      "lambda:*",
      "logs:*",
      "pipes:*",
      "s3:*",
      "scheduler:*",
      "sns:*",
      "sqs:*",
      "states:*",
      "sts:GetCallerIdentity",
      "tag:GetResources",
      "wafv2:*",
      "xray:*",
    ]
    resources = ["*"]
  }

  statement {
    sid = "AllowIamRead"
    actions = [
      "iam:Get*",
      "iam:List*",
    ]
    resources = ["*"]
  }

  statement {
    sid = "AllowIamOnProjectRolesAndPolicies"
    actions = [
      "iam:*Role",
      "iam:*RolePolicy",
      "iam:*RolePermissionsBoundary",
      "iam:UpdateRoleDescription",
      "iam:UpdateAssumeRolePolicy",
      "iam:TagRole",
      "iam:UntagRole",
      "iam:PassRole",
      "iam:*Policy",
      "iam:*PolicyVersion",
      "iam:TagPolicy",
      "iam:UntagPolicy",
      "iam:SimulatePrincipalPolicy",
    ]
    resources = [
      "${local.iam_prefix}:role/dam-*",
      "${local.iam_prefix}:policy/dam-*",
    ]
  }

  statement {
    sid       = "DenyOutsideProjectRegions"
    effect    = "Deny"
    actions   = ["*"]
    resources = ["*"]

    condition {
      test     = "StringNotEquals"
      variable = "aws:RequestedRegion"
      values   = distinct([var.region, "us-east-1"])
    }
  }

  statement {
    sid    = "DenyRolesWithoutThisBoundary"
    effect = "Deny"
    actions = [
      "iam:CreateRole",
      "iam:PutRolePermissionsBoundary",
    ]
    resources = ["*"]

    condition {
      test     = "StringNotEquals"
      variable = "iam:PermissionsBoundary"
      values   = [local.permissions_boundary_arn]
    }
  }

  statement {
    sid       = "DenyBoundaryRemoval"
    effect    = "Deny"
    actions   = ["iam:DeleteRolePermissionsBoundary"]
    resources = ["*"]
  }

  statement {
    sid    = "DenyBoundaryPolicyChanges"
    effect = "Deny"
    actions = [
      "iam:CreatePolicyVersion",
      "iam:DeletePolicy",
      "iam:DeletePolicyVersion",
      "iam:SetDefaultPolicyVersion",
      "iam:TagPolicy",
      "iam:UntagPolicy",
    ]
    resources = [local.permissions_boundary_arn]
  }

  statement {
    sid    = "DenyGitHubRoleChanges"
    effect = "Deny"
    not_actions = [
      "iam:Get*",
      "iam:List*",
    ]
    resources = [
      "${local.iam_prefix}:role/dam-github-*",
      "${local.iam_prefix}:policy/dam-github-*",
    ]
  }

  statement {
    sid    = "ProtectTerraformState"
    effect = "Deny"
    actions = [
      "s3:DeleteBucket",
      "s3:DeleteBucketPolicy",
      "s3:PutBucketPolicy",
      "s3:PutBucketVersioning",
      "s3:PutLifecycleConfiguration",
      "s3:PutEncryptionConfiguration",
      "s3:PutBucketPublicAccessBlock",
    ]
    resources = ["arn:${data.aws_partition.current.partition}:s3:::${local.state_bucket_name}"]
  }
}

resource "aws_iam_policy" "permissions_boundary" {
  name        = local.permissions_boundary_name
  description = "Permission boundary dla wszystkich ról projektu Matchday DAM"
  policy      = data.aws_iam_policy_document.permissions_boundary.json
}
