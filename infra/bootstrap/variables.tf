variable "region" {
  description = "Region AWS, w którym działa cały projekt."
  type        = string
  default     = "eu-central-1"
}

variable "project" {
  description = "Prefiks nazw zasobów."
  type        = string
  default     = "matchday-dam"
}

variable "github_repository" {
  description = "Repozytorium GitHub w formacie owner/name (wielkość liter jak w URL-u repozytorium)."
  type        = string
  default     = "dawidbis/AWS_DAM_SPA"

  validation {
    condition     = can(regex("^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$", var.github_repository))
    error_message = "Podaj repozytorium jako owner/name."
  }
}

variable "deploy_branch" {
  description = "Gałąź, z której GitHub Actions może przyjąć rolę deployu."
  type        = string
  default     = "main"
}

variable "budget_alert_email" {
  description = "Adres e-mail, na który AWS Budgets wysyła alerty kosztowe."
  type        = string
}

variable "budget_limits_usd" {
  description = "Progi miesięcznych kosztów (USD), dla których powstają osobne budżety z alertami."
  type        = list(number)
  default     = [5, 20]
}
