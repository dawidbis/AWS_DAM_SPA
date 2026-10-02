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
