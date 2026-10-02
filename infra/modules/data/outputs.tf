output "assets_table_name" {
  value = aws_dynamodb_table.assets.name
}

output "assets_table_arn" {
  value = aws_dynamodb_table.assets.arn
}
