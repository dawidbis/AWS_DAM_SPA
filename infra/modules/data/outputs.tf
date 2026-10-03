output "assets_table_name" {
  value = aws_dynamodb_table.assets.name
}

output "assets_table_arn" {
  value = aws_dynamodb_table.assets.arn
}

output "incidents_table_name" {
  value = aws_dynamodb_table.incidents.name
}

output "incidents_table_arn" {
  value = aws_dynamodb_table.incidents.arn
}
