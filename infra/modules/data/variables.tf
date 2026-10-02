variable "name_prefix" {
  type = string
}

variable "deletion_protection" {
  description = "Blokada usunięcia tabeli (w dev wyłączona, żeby działał terraform destroy)."
  type        = bool
  default     = false
}
