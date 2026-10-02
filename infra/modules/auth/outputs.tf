output "user_pool_id" {
  value = aws_cognito_user_pool.this.id
}

output "user_pool_arn" {
  value = aws_cognito_user_pool.this.arn
}

output "issuer_url" {
  description = "Issuer tokenów JWT (do autoryzatora API Gateway i konfiguracji OIDC)."
  value       = "https://${aws_cognito_user_pool.this.endpoint}"
}

output "client_id" {
  value = aws_cognito_user_pool_client.spa.id
}

output "domain_url" {
  description = "Adres managed login."
  value       = "https://${aws_cognito_user_pool_domain.this.domain}.auth.${aws_cognito_user_pool.this.region}.amazoncognito.com"
}

output "group_names" {
  value = sort(keys(aws_cognito_user_group.this))
}
