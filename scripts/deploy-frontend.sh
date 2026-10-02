#!/usr/bin/env bash
# Buduje Angulara i wgrywa go do bucketu frontendu, potem unieważnia
# index.html w CloudFront. config.json zarządza Terraform, więc go pomijamy.
set -euo pipefail

TF_DIR="${TF_DIR:-infra/envs/dev}"
DIST="frontend/dist/matchday-dam/browser"

bucket=$(terraform -chdir="$TF_DIR" output -raw frontend_bucket)
distribution=$(terraform -chdir="$TF_DIR" output -raw frontend_distribution_id)

(cd frontend && npm ci && npx ng build)

# Pliki JS/CSS mają hash w nazwie, więc mogą być cache'owane bezterminowo.
# Bez --delete: przeglądarka ze starym index.html dalej znajdzie swoje chunki.
aws s3 sync "$DIST" "s3://$bucket" \
  --exclude "*" --include "*.js" --include "*.css" \
  --cache-control "public,max-age=31536000,immutable"

# Reszta (index.html, favicon) zawsze rewalidowana; config.json zostaje nietknięty.
aws s3 sync "$DIST" "s3://$bucket" --delete \
  --exclude "*.js" --exclude "*.css" --exclude "config.json" \
  --cache-control "no-cache"

aws cloudfront create-invalidation --distribution-id "$distribution" \
  --paths "/index.html" "/" --query "Invalidation.Id" --output text
