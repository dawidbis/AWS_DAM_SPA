#!/usr/bin/env bash
# Zakłada konto w Cognito i dodaje je do grupy. Cognito wysyła na podany
# adres zaproszenie z hasłem tymczasowym (zmiana przy pierwszym logowaniu).
#
#   ./scripts/create-user.sh <email> <admin|staff|contributor|viewer>
#
# Wymaga sesji AWS administratora i zainicjalizowanego infra/envs/dev
# (just init) albo zmiennej USER_POOL_ID.
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "Użycie: $0 <email> <admin|staff|contributor|viewer>" >&2
  exit 1
fi

email="$1"
group="$2"

case "$group" in
  admin | staff | contributor | viewer) ;;
  *) echo "Nieznana grupa: $group" >&2; exit 1 ;;
esac

pool="${USER_POOL_ID:-$(terraform -chdir="${TF_DIR:-infra/envs/dev}" output -raw cognito_user_pool_id)}"

aws cognito-idp admin-create-user \
  --user-pool-id "$pool" \
  --username "$email" \
  --user-attributes Name=email,Value="$email" Name=email_verified,Value=true \
  --desired-delivery-mediums EMAIL \
  --query "User.Username" --output text

aws cognito-idp admin-add-user-to-group \
  --user-pool-id "$pool" \
  --username "$email" \
  --group-name "$group"

echo "Utworzono $email w grupie $group. Zaproszenie wysłane mailem."
