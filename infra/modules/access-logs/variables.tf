variable "bucket_name" {
  description = "Nazwa bucketu na logi dostępu."
  type        = string
}

variable "retention_days" {
  description = "Po ilu dniach logi są usuwane."
  type        = number
  default     = 30
}
