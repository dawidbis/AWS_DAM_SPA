output "bucket_id" {
  description = "Nazwa bucketu (target_bucket dla logów serwerowych S3)."
  value       = aws_s3_bucket.logs.id
}

output "bucket_domain_name" {
  description = "Domena bucketu (logging_config.bucket w CloudFront)."
  value       = aws_s3_bucket.logs.bucket_domain_name
}
