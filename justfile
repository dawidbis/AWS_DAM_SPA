# Matchday DAM: polecenia developerskie. Lista: `just --list`.

set shell := ["bash", "-euo", "pipefail", "-c"]

env := "dev"
tf_env := "infra/envs/" + env

default:
    @just --list

# --- build -----------------------------------------------------------------

# Buduje wszystko: Lambdy (arm64 zip) i frontend
build: build-lambdas build-frontend

# Buduje Lambdy w Ruście do lambdas/target/lambda/<crate>/bootstrap.zip
build-lambdas:
    cd lambdas && cargo lambda build --release --arm64 --output-format zip

# Buduje aplikację Angular do frontend/dist/
build-frontend:
    cd frontend && npm ci && npx ng build

# --- jakość ------------------------------------------------------------------

# Formatuje kod Rust, Terraform i frontend
fmt:
    cd lambdas && cargo fmt --all
    terraform fmt -recursive infra
    cd frontend && npx prettier --write "src/**/*.{ts,html,css}"

# To samo, co sprawdza CI
check: check-rust check-frontend check-infra

check-rust:
    cd lambdas && cargo fmt --all --check
    cd lambdas && cargo clippy --all-targets --locked -- -D warnings
    cd lambdas && cargo test --locked

check-frontend:
    cd frontend && npm ci && npx ng lint && npx ng test --watch=false

check-infra:
    terraform fmt -check -recursive infra
    terraform -chdir=infra/bootstrap init -backend=false -input=false > /dev/null
    terraform -chdir=infra/bootstrap validate
    terraform -chdir={{tf_env}} init -backend=false -input=false > /dev/null
    terraform -chdir={{tf_env}} validate
    tflint --init --config "$PWD/.tflint.hcl" && tflint --recursive --config "$PWD/.tflint.hcl"
    checkov -d infra --config-file .checkov.yaml

# --- AWS -------------------------------------------------------------------

# Jednorazowo: bucket stanu, OIDC GitHub, role CI, boundary, budżety (konto admina)
# (nowe konto: najpierw docs/setup-aws.md, kroki 3-4)
bootstrap:
    bucket="${TF_STATE_BUCKET:-matchday-dam-tfstate-$(aws sts get-caller-identity --query Account --output text)}"; \
    terraform -chdir=infra/bootstrap init -input=false -backend-config="bucket=${bucket}"
    terraform -chdir=infra/bootstrap apply

# Inicjalizuje backend środowiska (bucket = $TF_STATE_BUCKET albo wyliczony z ID konta)
init:
    bucket="${TF_STATE_BUCKET:-matchday-dam-tfstate-$(aws sts get-caller-identity --query Account --output text)}"; \
    terraform -chdir={{tf_env}} init -input=false -reconfigure -backend-config="bucket=${bucket}"

# terraform plan dla środowiska
plan: build-lambdas init
    terraform -chdir={{tf_env}} plan

# Build + terraform apply + frontend
deploy: build-lambdas init
    terraform -chdir={{tf_env}} apply
    TF_DIR={{tf_env}} ./scripts/deploy-frontend.sh

# Usuwa całe środowisko (awaryjny hamulec kosztów). Bootstrap zostaje.
destroy: init
    terraform -chdir={{tf_env}} destroy

# Build Angulara + upload do S3 + unieważnienie CloudFront
deploy-frontend: init
    TF_DIR={{tf_env}} ./scripts/deploy-frontend.sh

# Zapisuje frontend/public/config.json z outputów Terraform (dla `npm start`)
frontend-config: init
    terraform -chdir={{tf_env}} output -json frontend_config > frontend/public/config.json
    @echo "Zapisano frontend/public/config.json"

# Zakłada konto testowe w Cognito, np. `just create-user ja+admin@gmail.com admin`
create-user email group: init
    TF_DIR={{tf_env}} ./scripts/create-user.sh {{email}} {{group}}

# Wywołuje wdrożoną funkcję hello-world
invoke-hello name="Kibic":
    aws lambda invoke --function-name matchday-dam-{{env}}-hello-world \
        --cli-binary-format raw-in-base64-out \
        --payload '{"name": "{{name}}"}' /dev/stdout
