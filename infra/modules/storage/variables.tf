variable "name_prefix" {
  description = "Prefiks nazw bucketów (np. matchday-dam-dev)."
  type        = string
}

variable "bucket_access" {
  description = <<-EOT
    Role (nazwy, nie ARN-y), które mogą czytać i zapisywać obiekty w każdym
    buckecie. Wszyscy pozostali dostają Deny w polityce bucketu. Domyślne
    wartości to role z rozdziału 10.1; mogą jeszcze nie istnieć.
  EOT
  type = map(object({
    readers = list(string)
    writers = list(string)
  }))
  default = {
    quarantine = {
      readers = ["dam-validate", "dam-scan", "dam-cdr", "dam-handle-infected"]
      writers = ["dam-upload-init", "dam-upload-status", "dam-upload-complete"]
    }
    # dam-finalize-clean kopiuje wersję po CDR z clean/staging/ do clean/<id>,
    # a dam-renditions robi z niej miniaturę i podgląd (polityki IAM obu ról
    # ograniczają odczyt do staging/*).
    clean = {
      readers = ["dam-assets-read", "dam-finalize-clean", "dam-renditions"]
      writers = ["dam-finalize-clean", "dam-cdr"]
    }
    infected = {
      readers = []
      writers = ["dam-handle-infected"]
    }
    # Miniatury i podglądy ze znakiem wodnym (rozdział 9): zapis tylko renditions.
    renditions = {
      readers = ["dam-assets-read"]
      writers = ["dam-renditions"]
    }
  }

  validation {
    condition     = alltrue([for name in ["quarantine", "clean", "infected", "renditions"] : contains(keys(var.bucket_access), name)])
    error_message = "bucket_access musi zawierać quarantine, clean, infected i renditions."
  }
}

variable "upload_allowed_origins" {
  description = "Originy SPA, z których przeglądarka może wysyłać części pliku do kwarantanny."
  type        = list(string)
}

variable "log_bucket_id" {
  description = "Bucket na logi serwerowe S3 (moduł access-logs)."
  type        = string
}

variable "quarantine_retention_days" {
  description = "Po ilu dniach plik znika z kwarantanny (rozdział 9: 3–7 dni)."
  type        = number
  default     = 7
}

variable "infected_retention_days" {
  description = "Jak długo przechowywać dowody incydentów (lifecycle i domyślna retencja Object Lock)."
  type        = number
  default     = 90
}

variable "force_destroy" {
  description = "Pozwala usunąć buckety z zawartością (terraform destroy w dev)."
  type        = bool
  default     = true
}
