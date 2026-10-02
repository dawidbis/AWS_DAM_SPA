variable "name" {
  description = "Prefiks nazw zasobów (np. matchday-dam-dev)."
  type        = string
}

variable "bucket_name" {
  description = "Nazwa bucketu z plikami SPA."
  type        = string
}

variable "runtime_config" {
  description = "Zawartość /config.json czytanego przez Angulara przy starcie."
  type        = any
  default     = {}
}

variable "log_bucket_id" {
  description = "Bucket na logi serwerowe S3 (moduł access-logs)."
  type        = string
}

variable "log_bucket_domain_name" {
  description = "Domena bucketu na standardowe logi CloudFront (moduł access-logs)."
  type        = string
}
