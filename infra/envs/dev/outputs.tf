output "hello_world_function_name" {
  value = module.hello_world.function_name
}

output "frontend_url" {
  value = module.frontend.url
}

output "frontend_bucket" {
  value = module.frontend.bucket_name
}

output "frontend_distribution_id" {
  value = module.frontend.distribution_id
}

output "cognito_user_pool_id" {
  value = module.auth.user_pool_id
}

output "cognito_client_id" {
  value = module.auth.client_id
}

output "cognito_issuer_url" {
  value = module.auth.issuer_url
}

output "cognito_domain_url" {
  value = module.auth.domain_url
}

output "frontend_config" {
  description = "Zawartość config.json dla lokalnego `ng serve` (just frontend-config)."
  value       = local.frontend_config
}

output "api_url" {
  value = module.api.url
}

output "storage_buckets" {
  value = module.storage.bucket_names
}

output "assets_table" {
  value = module.data.assets_table_name
}
