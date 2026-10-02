variable "name" {
  description = "Krótka nazwa funkcji (np. upload-init). Rola dostaje nazwę dam-<name>."
  type        = string

  validation {
    condition     = can(regex("^[a-z0-9-]{1,48}$", var.name))
    error_message = "Nazwa: małe litery, cyfry i myślniki, maks. 48 znaków."
  }
}

variable "function_name" {
  description = "Pełna nazwa funkcji Lambda."
  type        = string
}

variable "description" {
  type    = string
  default = ""
}

variable "zip_path" {
  description = "Ścieżka do bootstrap.zip zbudowanego przez `cargo lambda build --output-format zip`."
  type        = string
}

variable "permissions_boundary_arn" {
  description = "Permission boundary z infra/bootstrap (wymagane dla każdej roli projektu)."
  type        = string
}

variable "policies" {
  description = "Dokumenty polityk IAM (JSON) tworzonych i przypinanych do roli funkcji: klucz => dokument."
  type        = map(string)
  default     = {}
}

variable "policy_arns" {
  description = "Dodatkowe polityki customer managed przypinane do roli funkcji."
  type        = map(string)
  default     = {}
}

variable "memory_size" {
  type    = number
  default = 128
}

variable "timeout" {
  type    = number
  default = 10
}

variable "reserved_concurrency" {
  description = "Limit współbieżności (-1 = bez limitu). Ogranicza koszty przy nadużyciach."
  type        = number
  default     = -1
}

variable "log_level" {
  type    = string
  default = "INFO"

  validation {
    condition     = contains(["TRACE", "DEBUG", "INFO", "WARN", "ERROR"], var.log_level)
    error_message = "Dozwolone poziomy: TRACE, DEBUG, INFO, WARN, ERROR."
  }
}

variable "log_retention_days" {
  type    = number
  default = 14
}

variable "environment" {
  description = "Dodatkowe zmienne środowiskowe funkcji."
  type        = map(string)
  default     = {}
}
