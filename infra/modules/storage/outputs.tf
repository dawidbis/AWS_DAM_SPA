output "bucket_names" {
  description = "Nazwy bucketów: quarantine, clean, infected."
  value       = { for name, bucket in aws_s3_bucket.this : name => bucket.id }
}

output "bucket_arns" {
  value = { for name, bucket in aws_s3_bucket.this : name => bucket.arn }
}
