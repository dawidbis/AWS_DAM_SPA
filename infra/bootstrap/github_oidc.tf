# Dostawca OIDC GitHub i dwie role dla GitHub Actions (rozdział 10.1):
#   - dam-github-plan:   tylko odczyt, dla `terraform plan` w pull requestach,
#   - dam-github-deploy: deploy po merge do gałęzi var.deploy_branch.
# Dzięki temu w repozytorium nie ma żadnych kluczy dostępowych AWS.

locals {
  github_oidc_host = "token.actions.githubusercontent.com"
  account_id       = data.aws_caller_identity.current.account_id
  iam_prefix       = "arn:${data.aws_partition.current.partition}:iam::${local.account_id}"

  # GitHub wystawia `sub` z niezmiennymi ID właściciela i repozytorium:
  #   repo:<owner>@<owner_id>/<repo>@<repo_id>:<kontekst>
  # Dzięki ID token nie pasuje do repozytorium, które ktoś założy pod tą samą
  # nazwą po usunięciu lub przemianowaniu oryginału. Faktyczny `sub` wypisuje
  # krok „Claimy tokenu OIDC" w .github/workflows/plan.yml.
  github_repo_parts = split("/", var.github_repository)
  github_sub_prefix = "repo:${local.github_repo_parts[0]}@${var.github_owner_id}/${local.github_repo_parts[1]}@${var.github_repository_id}"
}

resource "aws_iam_openid_connect_provider" "github" {
  url            = "https://${local.github_oidc_host}"
  client_id_list = ["sts.amazonaws.com"]
  # Od 2023 r. AWS weryfikuje certyfikat GitHub przez własne zaufane CA,
  # więc thumbprint_list nie jest wymagany.
}

# --- dam-github-plan -------------------------------------------------------

data "aws_iam_policy_document" "github_plan_trust" {
  statement {
    sid     = "GitHubActionsPullRequests"
    actions = ["sts:AssumeRoleWithWebIdentity"]

    principals {
      type        = "Federated"
      identifiers = [aws_iam_openid_connect_provider.github.arn]
    }

    condition {
      test     = "StringEquals"
      variable = "${local.github_oidc_host}:aud"
      values   = ["sts.amazonaws.com"]
    }

    condition {
      test     = "StringEquals"
      variable = "${local.github_oidc_host}:sub"
      values   = ["${local.github_sub_prefix}:pull_request"]
    }
  }
}

resource "aws_iam_role" "github_plan" {
  name                 = "dam-github-plan"
  description          = "GitHub Actions: terraform plan w pull requestach (tylko odczyt)"
  assume_role_policy   = data.aws_iam_policy_document.github_plan_trust.json
  permissions_boundary = aws_iam_policy.permissions_boundary.arn
  max_session_duration = 3600
}

resource "aws_iam_role_policy_attachment" "github_plan_readonly" {
  role       = aws_iam_role.github_plan.name
  policy_arn = "arn:${data.aws_partition.current.partition}:iam::aws:policy/ReadOnlyAccess"
}

data "aws_iam_policy_document" "github_plan_state_lock" {
  statement {
    sid = "ManageStateLockFiles"
    actions = [
      "s3:PutObject",
      "s3:DeleteObject",
    ]
    resources = ["${aws_s3_bucket.tfstate.arn}/*.tflock"]
  }
}

resource "aws_iam_policy" "github_plan_state_lock" {
  name        = "dam-github-plan-state-lock"
  description = "Zakładanie i zwalnianie blokady stanu Terraform podczas plan"
  policy      = data.aws_iam_policy_document.github_plan_state_lock.json
}

resource "aws_iam_role_policy_attachment" "github_plan_state_lock" {
  role       = aws_iam_role.github_plan.name
  policy_arn = aws_iam_policy.github_plan_state_lock.arn
}

# --- dam-github-deploy -----------------------------------------------------

data "aws_iam_policy_document" "github_deploy_trust" {
  statement {
    sid     = "GitHubActionsDeployBranch"
    actions = ["sts:AssumeRoleWithWebIdentity"]

    principals {
      type        = "Federated"
      identifiers = [aws_iam_openid_connect_provider.github.arn]
    }

    condition {
      test     = "StringEquals"
      variable = "${local.github_oidc_host}:aud"
      values   = ["sts.amazonaws.com"]
    }

    condition {
      test     = "StringEquals"
      variable = "${local.github_oidc_host}:sub"
      values   = ["${local.github_sub_prefix}:ref:refs/heads/${var.deploy_branch}"]
    }
  }
}

resource "aws_iam_role" "github_deploy" {
  name                 = "dam-github-deploy"
  description          = "GitHub Actions: build i terraform apply po merge do ${var.deploy_branch}"
  assume_role_policy   = data.aws_iam_policy_document.github_deploy_trust.json
  permissions_boundary = aws_iam_policy.permissions_boundary.arn
  max_session_duration = 3600
}

# PowerUserAccess daje pełny dostęp do usług poza IAM/Organizations.
# Faktyczny zakres jest przycięty przez permission boundary (lista usług,
# region, ochrona stanu i ról GitHub). Uzasadnienie: docs/adr/0013-github-oidc-deploy-roles.md.
resource "aws_iam_role_policy_attachment" "github_deploy_power_user" {
  role       = aws_iam_role.github_deploy.name
  policy_arn = "arn:${data.aws_partition.current.partition}:iam::aws:policy/PowerUserAccess"
}

data "aws_iam_policy_document" "github_deploy_iam" {
  #checkov:skip=CKV_AWS_356:iam:Get*/List* na "*" potrzebne do odczytu ról i polityk AWS managed przez terraform plan
  statement {
    sid = "ReadIam"
    actions = [
      "iam:Get*",
      "iam:List*",
    ]
    resources = ["*"]
  }

  statement {
    sid = "CreateProjectRolesWithBoundary"
    actions = [
      "iam:CreateRole",
      "iam:PutRolePermissionsBoundary",
    ]
    resources = ["${local.iam_prefix}:role/dam-*"]

    condition {
      test     = "StringEquals"
      variable = "iam:PermissionsBoundary"
      values   = [aws_iam_policy.permissions_boundary.arn]
    }
  }

  statement {
    sid = "ManageProjectRoles"
    actions = [
      "iam:DeleteRole",
      "iam:UpdateRole",
      "iam:UpdateRoleDescription",
      "iam:UpdateAssumeRolePolicy",
      "iam:TagRole",
      "iam:UntagRole",
      "iam:AttachRolePolicy",
      "iam:DetachRolePolicy",
      "iam:PutRolePolicy",
      "iam:DeleteRolePolicy",
    ]
    resources = ["${local.iam_prefix}:role/dam-*"]
  }

  statement {
    sid = "ManageProjectPolicies"
    actions = [
      "iam:CreatePolicy",
      "iam:CreatePolicyVersion",
      "iam:DeletePolicy",
      "iam:DeletePolicyVersion",
      "iam:SetDefaultPolicyVersion",
      "iam:TagPolicy",
      "iam:UntagPolicy",
    ]
    resources = ["${local.iam_prefix}:policy/dam-*"]
  }

  # Testy IAM w e2e (scenariusz 15): symulacja, czy rola projektu dostanie
  # AccessDenied. Tylko odczyt, bez zmiany uprawnień.
  statement {
    sid       = "SimulateProjectRoles"
    actions   = ["iam:SimulatePrincipalPolicy"]
    resources = ["${local.iam_prefix}:role/dam-*"]
  }

  statement {
    sid       = "PassProjectRolesToServices"
    actions   = ["iam:PassRole"]
    resources = ["${local.iam_prefix}:role/dam-*"]

    condition {
      test     = "StringEquals"
      variable = "iam:PassedToService"
      values = [
        "lambda.amazonaws.com",
        "states.amazonaws.com",
        "events.amazonaws.com",
        "scheduler.amazonaws.com",
        "apigateway.amazonaws.com",
      ]
    }
  }
}

resource "aws_iam_policy" "github_deploy_iam" {
  name        = "dam-github-deploy-iam"
  description = "Zarządzanie rolami i politykami dam-* (tylko z permission boundary)"
  policy      = data.aws_iam_policy_document.github_deploy_iam.json
}

resource "aws_iam_role_policy_attachment" "github_deploy_iam" {
  role       = aws_iam_role.github_deploy.name
  policy_arn = aws_iam_policy.github_deploy_iam.arn
}
