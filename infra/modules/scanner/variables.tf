variable "name_prefix" {
  type = string
}

variable "image_uri" {
  description = "Obraz skanera w ECR (repo@sha256:...). Pusty = Lambda jeszcze nie powstaje."
  type        = string
  default     = ""
}

variable "permissions_boundary_arn" {
  type = string
}

variable "assets_table_name" {
  type = string
}

variable "assets_table_arn" {
  type = string
}

variable "quarantine_bucket" {
  type = string
}

variable "quarantine_bucket_arn" {
  type = string
}

variable "clean_bucket" {
  type = string
}

variable "clean_bucket_arn" {
  type = string
}

variable "infected_bucket" {
  type = string
}

variable "infected_bucket_arn" {
  type = string
}

variable "alert_email" {
  description = "Adres alertów o zainfekowanych plikach (pusty = bez subskrypcji e-mail)."
  type        = string
  default     = ""
}

variable "log_retention_days" {
  type    = number
  default = 14
}
