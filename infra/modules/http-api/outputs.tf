output "api_id" {
  value = aws_apigatewayv2_api.this.id
}

output "url" {
  description = "Bazowy URL API (bez końcowego ukośnika)."
  value       = trimsuffix(aws_apigatewayv2_stage.default.invoke_url, "/")
}
