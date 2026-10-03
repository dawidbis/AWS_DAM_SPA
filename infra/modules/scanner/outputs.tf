output "repository_url" {
  value = aws_ecr_repository.this.repository_url
}

output "repository_name" {
  value = aws_ecr_repository.this.name
}

output "scan_queue_url" {
  value = aws_sqs_queue.scan.id
}

output "dlq_url" {
  value = aws_sqs_queue.dlq.id
}

output "alerts_topic_arn" {
  value = aws_sns_topic.alerts.arn
}

output "state_machine_arn" {
  description = "ARN maszyny stanów scan-pipeline (ponowienie skanu przez admina)."
  value       = local.state_machine_arn
}
