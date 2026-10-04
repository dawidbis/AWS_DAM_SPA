#!/usr/bin/env bash
# Testy e2e scenariuszy bezpieczeństwa z rozdziału 12 na środowisku dev.
#
#   ./tests/e2e/run.sh
#
# Wymaga: aws, curl, jq, terraform (zainicjalizowany infra/envs/dev) albo
# zmiennych środowiskowych z wartościami outputów (patrz `output` niżej).
# Uruchamiaj z poświadczeniami administratora (CloudShell) albo z workflow
# .github/workflows/e2e.yml (rola dam-github-deploy).
#
# Skrypt zakłada tymczasowych użytkowników w każdej grupie (A–D), wgrywa pliki
# z tests/security-fixtures i sprawdza wynik pipeline'u w DynamoDB, S3 i API.
# Na końcu usuwa użytkowników i dane testowe. Wyjątek: plik EICAR w buckecie
# infected (Object Lock) zostaje jako dowód, a na ALERT_EMAIL przychodzi alert.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
FIXTURES="$ROOT/tests/security-fixtures"
TF_DIR="${TF_DIR:-$ROOT/infra/envs/dev}"
RUN_ID="e2e$(date +%s)"
WORK="$(mktemp -d)"
TIMEOUT_S="${E2E_TIMEOUT_S:-420}"
HTTP_CODE='%{http_code}'

output() {
  # Wartość outputu Terraform (lub zmiennej środowiskowej o tej samej nazwie wielkimi literami).
  local name="$1" env_name
  env_name="$(echo "$name" | tr '[:lower:]' '[:upper:]')"
  if [[ -n "${!env_name:-}" ]]; then
    echo "${!env_name}"
  else
    terraform -chdir="$TF_DIR" output -raw "$name"
  fi
}

API="$(output api_url)"
POOL="$(output cognito_user_pool_id)"
CLIENT="$(output cognito_e2e_client_id)"
TABLE="$(output assets_table)"
INCIDENTS="$(output incidents_table)"
STATE_MACHINE="$(output scan_state_machine_arn)"
BUCKETS_JSON="${STORAGE_BUCKETS:-$(terraform -chdir="$TF_DIR" output -json storage_buckets)}"
bucket() {
  local name="$1"
  jq -r --arg name "$name" '.[$name]' <<<"$BUCKETS_JSON"
}

# object_count <bucket> <klucz> → ile obiektów ma dokładnie ten klucz (0 albo 1).
# Liczymy elementy Contents: CLI v2 stronicuje list-objects-v2 automatycznie
# i w scalonym wyniku nie ma pola KeyCount (zawsze null).
object_count() {
  local bucket_name="$1" key="$2"
  aws s3api list-objects-v2 --bucket "$bucket_name" --prefix "$key" \
    --query "length(Contents[?Key=='$key'] || \`[]\`)" --output text
}

PASSED=()
FAILED=()
ASSETS=()
USERS=()

pass() {
  local name="$1"
  PASSED+=("$name")
  echo "  ✔ $name"
}
fail() {
  local name="$1" reason="$2"
  FAILED+=("$name: $reason")
  echo "  ✘ $name: $reason"
}
check() { # check <nazwa> <warunek-jako-polecenie...>
  local name="$1"
  shift
  if "$@" >/dev/null 2>&1; then pass "$name"; else fail "$name" "warunek niespełniony: $*"; fi
}

# --- Sprzątanie ---------------------------------------------------------------

cleanup() {
  set +e
  echo
  echo "Sprzątanie…"
  for user in "${USERS[@]}"; do
    aws cognito-idp admin-delete-user --user-pool-id "$POOL" --username "$user" >/dev/null 2>&1
  done
  if [[ -f "$WORK/uploaded-assets" ]]; then
    mapfile -t -O "${#ASSETS[@]}" ASSETS <"$WORK/uploaded-assets"
  fi
  for asset in "${ASSETS[@]}"; do
    aws dynamodb delete-item --table-name "$TABLE" --key "{\"pk\":{\"S\":\"ASSET#$asset\"}}" >/dev/null 2>&1
    aws dynamodb delete-item --table-name "$INCIDENTS" --key "{\"incidentId\":{\"S\":\"$asset\"}}" >/dev/null 2>&1
    for target in "quarantine:$asset" "clean:$asset" "clean:staging/$asset" "renditions:thumb/$asset.jpg" "renditions:preview/$asset.jpg"; do
      aws s3api delete-object --bucket "$(bucket "${target%%:*}")" --key "${target#*:}" >/dev/null 2>&1
    done
  done
  rm -rf "$WORK"
}
trap cleanup EXIT

# --- Użytkownicy i tokeny -------------------------------------------------------

declare -A TOKEN
create_user() {
  local group="$1" email password
  email="$RUN_ID-$group@example.invalid"
  password="E2e-$(openssl rand -hex 12)A1"
  aws cognito-idp admin-create-user --user-pool-id "$POOL" --username "$email" \
    --user-attributes Name=email,Value="$email" Name=email_verified,Value=true \
    --message-action SUPPRESS >/dev/null
  USERS+=("$email")
  aws cognito-idp admin-set-user-password --user-pool-id "$POOL" --username "$email" \
    --password "$password" --permanent
  aws cognito-idp admin-add-user-to-group --user-pool-id "$POOL" --username "$email" --group-name "$group"
  TOKEN[$group]="$(aws cognito-idp admin-initiate-auth --user-pool-id "$POOL" --client-id "$CLIENT" \
    --auth-flow ADMIN_USER_PASSWORD_AUTH --auth-parameters USERNAME="$email",PASSWORD="$password" \
    --query AuthenticationResult.AccessToken --output text)"
}

# api <grupa|-> <metoda> <ścieżka> [ciało] → plik z odpowiedzią, kod HTTP na stdout
api() {
  local group="$1" method="$2" path="$3" body="${4:-}" out="$WORK/response.json" auth=()
  [[ "$group" != "-" ]] && auth=(-H "authorization: Bearer ${TOKEN[$group]}")
  if [[ -n "$body" ]]; then
    curl -sS -o "$out" -w "$HTTP_CODE" -X "$method" "${auth[@]}" -H 'content-type: application/json' --data "$body" "$API$path"
  else
    curl -sS -o "$out" -w "$HTTP_CODE" -X "$method" "${auth[@]}" "$API$path"
  fi
}

# --- Upload i oczekiwanie na pipeline -----------------------------------------------

# upload <grupa> <plik> <content-type> [nazwa] → ID assetu na stdout
upload() {
  local group="$1" file="$2" type="$3" name="${4:-}" size code asset part url offset
  [[ -n "$name" ]] || name="$(basename "$file")"
  size="$(wc -c <"$file" | tr -d ' ')"
  code="$(api "$group" POST /uploads "$(jq -nc --arg f "$name" --argjson s "$size" --arg t "$type" '{filename:$f,size:$s,contentType:$t}')")"
  [[ "$code" == 201 || "$code" == 200 ]] || { echo "upload-init $code: $(cat "$WORK/response.json")" >&2; return 1; }
  asset="$(jq -r .assetId "$WORK/response.json")"
  # upload działa w podpowłoce ($(upload …)), więc ASSETS+= by przepadło:
  # ID trafia do pliku, który czyta sprzątanie (także po przerwaniu skryptu).
  echo "$asset" >>"$WORK/uploaded-assets"
  local part_size
  part_size="$(jq -r .partSize "$WORK/response.json")"
  jq -c '.parts[]' "$WORK/response.json" >"$WORK/parts"
  while read -r part; do
    url="$(jq -r .url <<<"$part")"
    offset=$(( ($(jq -r .partNumber <<<"$part") - 1) * part_size ))
    tail -c +$((offset + 1)) "$file" | head -c "$part_size" >"$WORK/part"
    curl -sSf -o /dev/null -X PUT --data-binary @"$WORK/part" "$url"
  done <"$WORK/parts"
  code="$(api "$group" POST "/uploads/$asset/complete" '{}')"
  [[ "$code" == 200 ]] || { echo "upload-complete $code: $(cat "$WORK/response.json")" >&2; return 1; }
  echo "$asset"
}

status_of() {
  local asset="$1"
  aws dynamodb get-item --table-name "$TABLE" --key "{\"pk\":{\"S\":\"ASSET#$asset\"}}" --consistent-read \
    --query 'Item.status.S' --output text
}

attribute_of() {
  local asset="$1" attribute="$2"
  aws dynamodb get-item --table-name "$TABLE" --key "{\"pk\":{\"S\":\"ASSET#$asset\"}}" --consistent-read \
    --query "Item.$attribute.S" --output text
}

# wait_final <asset> → końcowy status po pipeline'ie
wait_final() {
  local asset="$1" deadline=$((SECONDS + TIMEOUT_S)) status
  while ((SECONDS < deadline)); do
    status="$(status_of "$asset")"
    case "$status" in
      CLEAN_DRAFT | INFECTED | REJECTED | SCAN_FAILED) echo "$status"; return ;;
      *) ;; # pipeline jeszcze pracuje
    esac
    sleep 5
  done
  echo "TIMEOUT($status)"
}

download() { # download <asset> <plik>: oryginał po CDR przez API (A)
  local asset="$1" target="$2" code
  code="$(api admin GET "/assets/$asset/download")"
  [[ "$code" == 200 ]] || return 1
  curl -sSf -o "$target" "$(jq -r .url "$WORK/response.json")"
}

contains() {
  local file="$1" needle="$2"
  grep -aqF -- "$needle" "$file"
}

# --- Scenariusze -----------------------------------------------------------------

echo "Przygotowanie użytkowników ($RUN_ID)…"
for group in admin staff contributor viewer; do create_user "$group"; done

echo "Upload plików testowych…"
# shellcheck disable=SC2016 # ciąg EICAR zawiera $ i ma zostać dosłowny
printf '%s' 'X5O!P%@AP[4\PZX54(P^)7CC)7}$EICAR-STANDARD-ANTIVIRUS-TEST-FILE!$H+H*' >"$WORK/eicar.jpg"
eicar="$(upload contributor "$WORK/eicar.jpg" image/jpeg)"
exif="$(upload contributor "$FIXTURES/exif-xss.jpg" image/jpeg)"
svg="$(upload contributor "$FIXTURES/svg-script.svg" image/png)"
exe="$(upload contributor "$FIXTURES/exe-renamed.jpg" image/jpeg)"
polyglot="$(upload contributor "$FIXTURES/polyglot.png" image/png)"
bomb="$(upload contributor "$FIXTURES/bomb.png" image/png)"
traversal="$(upload contributor "$FIXTURES/photo.jpg" image/jpeg '../../etc/passwd.jpg')"

echo "Scenariusze rozdziału 12:"

# 1. EICAR
check "1. EICAR → INFECTED" test "$(wait_final "$eicar")" = INFECTED
check "1. EICAR w buckecie infected" test "$(object_count "$(bucket infected)" "$eicar")" = 1
check "1. wpis w incidents" test "$(aws dynamodb get-item --table-name "$INCIDENTS" --key "{\"incidentId\":{\"S\":\"$eicar\"}}" --query 'Item.signature.S' --output text)" != None

# 2. XSS w EXIF
check "2. JPEG z XSS w EXIF → CLEAN_DRAFT" test "$(wait_final "$exif")" = CLEAN_DRAFT
if download "$exif" "$WORK/exif-clean.jpg"; then
  if contains "$WORK/exif-clean.jpg" '<script>' || contains "$WORK/exif-clean.jpg" 'Exif'; then
    fail "2. wersja po CDR bez EXIF" "plik nadal zawiera EXIF/skrypt"
  else
    pass "2. wersja po CDR bez EXIF"
  fi
else
  fail "2. wersja po CDR bez EXIF" "nie udało się pobrać pliku"
fi

# 3. SVG
check "3. SVG odrzucony już przy upload-init" test "$(api contributor POST /uploads '{"filename":"a.svg","size":95,"contentType":"image/svg+xml"}')" = 400
check "3. SVG zadeklarowany jako PNG → REJECTED" test "$(wait_final "$svg")" = REJECTED

# 4. Plik wykonywalny jako .jpg
check "4. .exe jako .jpg → REJECTED" test "$(wait_final "$exe")" = REJECTED

# 5. Poliglota
check "5. poliglota PNG+HTML → CLEAN_DRAFT" test "$(wait_final "$polyglot")" = CLEAN_DRAFT
if download "$polyglot" "$WORK/polyglot-clean.png" && ! contains "$WORK/polyglot-clean.png" '<html>'; then
  pass "5. po CDR tylko obraz"
else
  fail "5. po CDR tylko obraz" "plik zawiera doklejony HTML albo nie da się go pobrać"
fi

# 6. Bomba dekompresyjna
check "6. bomba → REJECTED" test "$(wait_final "$bomb")" = REJECTED
check "6. odrzucona na limicie wymiarów" bash -c "[[ '$(attribute_of "$bomb" rejectReason)' == *limit* ]]"

# 7. Path traversal w nazwie
check "7. ../../etc/passwd.jpg → CLEAN_DRAFT" test "$(wait_final "$traversal")" = CLEAN_DRAFT
check "7. nazwa po sanityzacji" test "$(attribute_of "$traversal" originalFilename)" = passwd.jpg
check "7. klucz S3 to UUID" test "$(object_count "$(bucket clean)" "$traversal")" = 1

# 8. <script> w tytule
check "8. tytuł ze skryptem → 400" test "$(api contributor POST /uploads '{"filename":"a.jpg","size":10,"contentType":"image/jpeg","title":"<script>alert(1)</script>"}')" = 400

# 9, 10. Uprawnienia grup
check "9. C publikuje → 403" test "$(api contributor POST "/assets/$traversal/publish" '{}')" = 403
check "10. D prosi o oryginał → 403" test "$(api viewer GET "/assets/$traversal/download")" = 403
check "10. D w galerii widzi tylko podglądy" bash -c "[[ \$(curl -sS -H 'authorization: Bearer ${TOKEN[viewer]}' '$API/assets?view=gallery' | jq '[.items[] | select((.previewUrl // \"\") | contains(\"/preview/\") | not)] | length') == 0 ]]"

# 11. Faktyczny rozmiar większy niż deklaracja
code="$(api contributor POST /uploads '{"filename":"liar.jpg","size":1000,"contentType":"image/jpeg"}')"
liar="$(jq -r .assetId "$WORK/response.json")"
ASSETS+=("$liar")
head -c 5000 /dev/urandom >"$WORK/big"
curl -sS -o /dev/null -X PUT --data-binary @"$WORK/big" "$(jq -r '.parts[0].url' "$WORK/response.json")"
check "11. upload-complete odrzuca większy plik" bash -c "[[ \$(curl -sS -o /dev/null -w '$HTTP_CODE' -X POST -H 'authorization: Bearer ${TOKEN[contributor]}' -H 'content-type: application/json' --data '{}' '$API/uploads/$liar/complete') == 4* ]]"
check "11. status REJECTED" test "$(status_of "$liar")" = REJECTED
check "11. obiekt usunięty z kwarantanny" test "$(object_count "$(bucket quarantine)" "$liar")" = 0

# 12. Upload na inny klucz niż w presigned URL
api contributor POST /uploads '{"filename":"x.jpg","size":100,"contentType":"image/jpeg"}' >/dev/null
other="$(jq -r .assetId "$WORK/response.json")"
ASSETS+=("$other")
forged="$(jq -r '.parts[0].url' "$WORK/response.json" | sed -E "s#/$other\\?#/00000000-0000-4000-8000-000000000000?#")"
check "12. inny klucz → błąd podpisu S3" test "$(curl -sS -o /dev/null -w "$HTTP_CODE" -X PUT --data-binary 'x' "$forged")" = 403

# 13. Podwójne zdarzenie S3
if aws stepfunctions start-execution --state-machine-arn "$STATE_MACHINE" --name "$exif" \
  --input "{\"assetId\":\"$exif\"}" >/dev/null 2>"$WORK/err"; then
  fail "13. drugie wykonanie dla tego samego assetu" "Step Functions uruchomił duplikat"
elif grep -q ExecutionAlreadyExists "$WORK/err"; then
  pass "13. drugie wykonanie dla tego samego assetu odrzucone"
else
  fail "13. drugie wykonanie dla tego samego assetu" "$(cat "$WORK/err")"
fi

# 14. Błąd skanera → SCAN_FAILED (asset bez pliku w kwarantannie)
ghost="$(cat /proc/sys/kernel/random/uuid)"
ASSETS+=("$ghost")
now="$(date +%s000)"
aws dynamodb put-item --table-name "$TABLE" --item "{\"pk\":{\"S\":\"ASSET#$ghost\"},\"assetId\":{\"S\":\"$ghost\"},\"status\":{\"S\":\"SCANNING\"},\"uploaderId\":{\"S\":\"$RUN_ID\"},\"createdAt\":{\"N\":\"$now\"}}"
aws stepfunctions start-execution --state-machine-arn "$STATE_MACHINE" --name "$RUN_ID-ghost" \
  --input "{\"assetId\":\"$ghost\",\"marked\":true}" >/dev/null
check "14. błąd skanu → SCAN_FAILED" test "$(wait_final "$ghost")" = SCAN_FAILED

# 15. Polityki IAM i bucketów: role nie mają dostępu poza swoim zakresem
account="$(aws sts get-caller-identity --query Account --output text)"
simulate() { # simulate <rola> <akcja> <zasób-arn> → allowed / implicitDeny / explicitDeny
  local role="$1" action="$2" resource="$3"
  aws iam simulate-principal-policy --policy-source-arn "arn:aws:iam::$account:role/$role" \
    --action-names "$action" --resource-arns "$resource" --query 'EvaluationResults[0].EvalDecision' --output text
}
if simulate dam-asset-publish s3:GetObject "arn:aws:s3:::$(bucket quarantine)/x" >"$WORK/sim" 2>"$WORK/err"; then
  for case in \
    "dam-asset-publish s3:GetObject arn:aws:s3:::$(bucket quarantine)/x" \
    "dam-assets-read s3:GetObject arn:aws:s3:::$(bucket quarantine)/x" \
    "dam-scan s3:PutObject arn:aws:s3:::$(bucket clean)/x" \
    "dam-upload-init s3:GetObject arn:aws:s3:::$(bucket clean)/x" \
    "dam-renditions s3:GetObject arn:aws:s3:::$(bucket quarantine)/x" \
    "dam-cdr s3:PutObject arn:aws:s3:::$(bucket infected)/x"; do
    read -r role action resource <<<"$case"
    check "15. $role $action poza zakresem → AccessDenied" bash -c "[[ \$(aws iam simulate-principal-policy --policy-source-arn arn:aws:iam::$account:role/$role --action-names $action --resource-arns $resource --query 'EvaluationResults[0].EvalDecision' --output text) == *Deny* ]]"
  done
else
  echo "  ! 15. pominięty: brak iam:SimulatePrincipalPolicy (zastosuj bootstrap, docs/setup-aws.md krok 13)"
fi

# 16. API bez tokenu i z obcym tokenem
check "16. bez tokenu → 401" test "$(api - GET /me)" = 401
forged_jwt="$(printf '{"alg":"RS256","kid":"x"}' | base64 | tr -d '=\n' | tr '/+' '_-').$(printf '{"sub":"x","cognito:groups":["admin"],"iss":"https://cognito-idp.eu-central-1.amazonaws.com/eu-central-1_FAKE"}' | base64 | tr -d '=\n' | tr '/+' '_-').c2lnbmF0dXJl"
check "16. token innej puli → 401" test "$(curl -sS -o /dev/null -w "$HTTP_CODE" -H "authorization: Bearer $forged_jwt" "$API/me")" = 401

# Usuwanie assetów (A): dowód incydentu zostaje, C nie może usuwać.
check "Usuwanie: C nie usunie assetu → 403" test "$(api contributor DELETE "/assets/$traversal")" = 403
check "Usuwanie: zainfekowany asset zostaje → 409" test "$(api admin DELETE "/assets/$eicar")" = 409
check "Usuwanie: A usuwa asset → 200" test "$(api admin DELETE "/assets/$traversal")" = 200
check "Usuwanie: rekord usunięty" test "$(status_of "$traversal")" = None
check "Usuwanie: plik usunięty z clean" test "$(object_count "$(bucket clean)" "$traversal")" = 0

echo
echo "Wynik: ${#PASSED[@]} OK, ${#FAILED[@]} błędów."
if ((${#FAILED[@]})); then
  printf '  ✘ %s\n' "${FAILED[@]}"
  exit 1
fi
