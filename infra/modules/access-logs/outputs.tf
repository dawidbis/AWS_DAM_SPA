output "bucket_id" {
  description = "Nazwa bucketu (target_bucket dla logów serwerowych S3)."
  value       = aws_s3_bucket.logs.id

  # Logi S3 zapisują przez politykę bucketu, więc konsument musi poczekać na nią.
  depends_on = [aws_s3_bucket_policy.logs]
}

output "bucket_domain_name" {
  description = "Domena bucketu (logging_config.bucket w CloudFront)."
  value       = aws_s3_bucket.logs.bucket_domain_name

  # CloudFront odrzuca bucket bez włączonych ACL (InvalidArgument), dlatego
  # dystrybucja może powstać dopiero po ustawieniu ownership controls i ACL.
  depends_on = [
    aws_s3_bucket_ownership_controls.logs,
    aws_s3_bucket_acl.logs,
  ]
}
