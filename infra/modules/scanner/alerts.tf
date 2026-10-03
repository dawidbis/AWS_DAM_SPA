# Alerty bezpieczeństwa: handle-infected publikuje zdarzenie domenowe
# asset.infected w EventBridge, a reguła poniżej zamienia je na mail SNS
# (rozdział 9). Kolejni odbiorcy (np. Slack, panel incydentów) to nowe reguły,
# bez zmian w Lambdzie.

# Bez SSE: alert zawiera tylko ID assetu, sub uploadera, IP i nazwę sygnatury
# (bez treści pliku i metadanych od użytkownika). Szyfrowanie KMS wymagałoby
# uprawnień kms w permission boundary i stałego kosztu klucza (rozdział 7.3).
resource "aws_sns_topic" "alerts" { # NOSONAR
  #checkov:skip=CKV_AWS_26:Alert zawiera tylko referencje i nazwę sygnatury; KMS wymaga zmiany boundary i kosztu klucza
  name = "${var.name_prefix}-security-alerts"
}

resource "aws_sns_topic_subscription" "email" {
  count = var.alert_email == "" ? 0 : 1

  topic_arn = aws_sns_topic.alerts.arn
  protocol  = "email"
  endpoint  = var.alert_email
}


resource "aws_cloudwatch_event_rule" "asset_infected" {
  name        = "${var.name_prefix}-asset-infected"
  description = "Zdarzenie asset.infected z handle-infected → alert SNS"
  event_pattern = jsonencode({
    source        = [local.event_source]
    "detail-type" = ["asset.infected"]
  })
}

resource "aws_cloudwatch_event_target" "alert_email" {
  rule = aws_cloudwatch_event_rule.asset_infected.name
  arn  = aws_sns_topic.alerts.arn

  input_transformer {
    input_paths = {
      asset     = "$.detail.assetId"
      uploader  = "$.detail.uploaderId"
      ip        = "$.detail.sourceIp"
      signature = "$.detail.signature"
      engine    = "$.detail.engine"
    }
    input_template = "\"Wykryto złośliwy plik w Matchday DAM.\\n\\nAsset: <asset>\\nUploader (sub): <uploader>\\nIP: <ip>\\nSygnatura: <signature>\\nSilnik: <engine>\\n\\nPlik przeniesiono do bucketu infected (Object Lock), incydent zapisano w tabeli incidents. Plik nie jest dostępny w galerii.\""
  }
}

# Publikować w temacie może tylko reguła asset.infected (i role konta przez IAM).
data "aws_iam_policy_document" "alerts" {
  statement {
    sid       = "AllowAssetInfectedRule"
    actions   = ["sns:Publish"]
    resources = [aws_sns_topic.alerts.arn]

    principals {
      type        = "Service"
      identifiers = ["events.amazonaws.com"]
    }

    condition {
      test     = "ArnEquals"
      variable = "aws:SourceArn"
      values   = [aws_cloudwatch_event_rule.asset_infected.arn]
    }
  }

  statement {
    sid       = "DenyInsecureTransport"
    effect    = "Deny"
    actions   = ["sns:Publish"]
    resources = [aws_sns_topic.alerts.arn]

    principals {
      type        = "*"
      identifiers = ["*"]
    }

    condition {
      test     = "Bool"
      variable = "aws:SecureTransport"
      values   = ["false"]
    }
  }
}

resource "aws_sns_topic_policy" "alerts" {
  arn    = aws_sns_topic.alerts.arn
  policy = data.aws_iam_policy_document.alerts.json
}
