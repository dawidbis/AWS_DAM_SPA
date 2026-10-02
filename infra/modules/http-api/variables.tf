variable "name" {
  type = string
}

variable "allowed_origins" {
  description = "Originy SPA dopuszczone przez CORS."
  type        = list(string)
}

variable "jwt_issuer" {
  description = "Issuer tokenów (User Pool Cognito)."
  type        = string
}

variable "jwt_audience" {
  description = "Dozwolone client_id aplikacji Cognito."
  type        = list(string)
}

variable "routes" {
  description = "Trasy API: route key (np. \"GET /me\") => funkcja Lambda."
  type = map(object({
    function_name = string
    function_arn  = string
  }))
}

variable "throttling_burst_limit" {
  type    = number
  default = 20
}

variable "throttling_rate_limit" {
  type    = number
  default = 10
}

variable "log_retention_days" {
  type    = number
  default = 14
}
