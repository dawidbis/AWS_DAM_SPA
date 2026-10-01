# Alerty kosztowe (etap 0). Każdy próg to osobny budżet, żeby mail
# jasno mówił, który limit został przekroczony.

resource "aws_budgets_budget" "monthly" {
  for_each = toset([for limit in var.budget_limits_usd : tostring(limit)])

  name         = "${var.project}-monthly-${each.value}usd"
  budget_type  = "COST"
  limit_amount = each.value
  limit_unit   = "USD"
  time_unit    = "MONTHLY"

  notification {
    comparison_operator        = "GREATER_THAN"
    notification_type          = "FORECASTED"
    threshold                  = 100
    threshold_type             = "PERCENTAGE"
    subscriber_email_addresses = [var.budget_alert_email]
  }

  notification {
    comparison_operator        = "GREATER_THAN"
    notification_type          = "ACTUAL"
    threshold                  = 80
    threshold_type             = "PERCENTAGE"
    subscriber_email_addresses = [var.budget_alert_email]
  }

  notification {
    comparison_operator        = "GREATER_THAN"
    notification_type          = "ACTUAL"
    threshold                  = 100
    threshold_type             = "PERCENTAGE"
    subscriber_email_addresses = [var.budget_alert_email]
  }
}
