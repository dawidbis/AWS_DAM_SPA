variable "name" {
  description = "Nazwa User Poola."
  type        = string
}

variable "domain_prefix" {
  description = "Prefiks domeny managed login (<prefix>.auth.<region>.amazoncognito.com), unikalny w regionie."
  type        = string
}

variable "callback_urls" {
  description = "Dozwolone adresy powrotu po logowaniu."
  type        = list(string)
}

variable "logout_urls" {
  description = "Dozwolone adresy powrotu po wylogowaniu."
  type        = list(string)
}

variable "groups" {
  description = "Grupy użytkowników: nazwa => opis i priorytet (niższy = ważniejszy)."
  type = map(object({
    description = string
    precedence  = number
  }))
}

variable "deletion_protection" {
  description = "Blokada usunięcia User Poola (w dev wyłączona, żeby działał terraform destroy)."
  type        = bool
  default     = false
}
