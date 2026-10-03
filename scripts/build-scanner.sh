#!/usr/bin/env bash
# Buduje i wypycha obraz skanera ClamAV do ECR, a URI z digestem zapisuje
# do $GITHUB_ENV jako TF_VAR_scanner_image_uri (albo wypisuje lokalnie).
#
# Obraz jest przebudowywany tylko, gdy: zmienił się kod skanera, w ECR nie ma
# jeszcze obrazu albo FORCE_SCANNER_BUILD=true (cotygodniowe odświeżenie
# sygnatur). W pozostałych przypadkach używany jest najnowszy obraz, żeby nie
# pobierać bazy ClamAV (~300 MB) przy każdym deployu.
#
# --check: tylko decyzja. Zapisuje rebuild=true|false do $GITHUB_OUTPUT (deploy
# instaluje toolchain Rusta wyłącznie, gdy obraz trzeba przebudować), a przy
# rebuild=false od razu publikuje URI najnowszego obrazu.
set -euo pipefail

check_only=false
[[ "${1:-}" == "--check" ]] && check_only=true

REPO_NAME="${SCANNER_REPOSITORY:-matchday-dam-dev-scanner}"
CONTEXT="lambdas/pipeline/scan/container"

repo_uri=$(aws ecr describe-repositories --repository-names "$REPO_NAME" \
  --query 'repositories[0].repositoryUri' --output text)
latest=$(aws ecr describe-images --repository-name "$REPO_NAME" \
  --query 'sort_by(imageDetails,&imagePushedAt)[-1].imageDigest' --output text 2>/dev/null || echo None)

changed=true
if git rev-parse -q --verify HEAD~1 > /dev/null \
  && git diff --quiet HEAD~1 HEAD -- lambdas/pipeline/scan lambdas/shared lambdas/Cargo.lock; then
  changed=false
fi

publish() {
  echo "Obraz skanera: $1"
  if [[ -n "${GITHUB_ENV:-}" ]]; then
    echo "TF_VAR_scanner_image_uri=$1" >> "$GITHUB_ENV"
  fi
}

rebuild=true
if [[ "${FORCE_SCANNER_BUILD:-false}" != "true" && "$changed" == "false" && "$latest" != "None" ]]; then
  rebuild=false
fi
if [[ "$check_only" == "true" && -n "${GITHUB_OUTPUT:-}" ]]; then
  echo "rebuild=$rebuild" >> "$GITHUB_OUTPUT"
fi
if [[ "$rebuild" == "false" ]]; then
  publish "$repo_uri@$latest"
  exit 0
fi
if [[ "$check_only" == "true" ]]; then
  echo "Obraz skanera do przebudowy."
  exit 0
fi

# Binarka `scan` (x86_64, jak obraz) implementuje Lambda Runtime API.
(cd lambdas && cargo lambda build --release --x86-64 -p pipeline-scan --lambda-dir target/scanner)
cp lambdas/target/scanner/scan/bootstrap "$CONTEXT/bootstrap"

aws ecr get-login-password | docker login --username AWS --password-stdin "${repo_uri%%/*}"
tag="$(git rev-parse --short HEAD)-$(date -u +%Y%m%d%H%M)"
docker build --platform linux/amd64 --provenance=false -t "$repo_uri:$tag" "$CONTEXT"
docker push "$repo_uri:$tag"
digest=$(aws ecr describe-images --repository-name "$REPO_NAME" --image-ids imageTag="$tag" \
  --query 'imageDetails[0].imageDigest' --output text)
publish "$repo_uri@$digest"
