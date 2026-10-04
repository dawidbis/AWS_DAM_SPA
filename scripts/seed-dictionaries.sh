#!/usr/bin/env bash
# Początkowe słowniki klubu (fikcyjna kadra, terminarz, sponsorzy) z
# scripts/seed/dictionaries.json, wgrywane TYLKO do pustej tabeli. Dzięki temu
# deploy może uruchamiać skrypt za każdym razem: po pierwszym wypełnieniu
# niczego nie nadpisuje ani nie przywraca wpisów usuniętych przez A.
#
#   ./scripts/seed-dictionaries.sh [nazwa-tabeli]
#
# Bez argumentu tabela z outputu Terraform (infra/envs/dev, `just init`).
# Format rekordu jak w shared::dictionary: kind (PLAYER, …), id, data (JSON).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SEED="$ROOT/scripts/seed/dictionaries.json"
table="${1:-$(terraform -chdir="$ROOT/infra/envs/dev" output -raw dictionaries_table)}"

count="$(aws dynamodb scan --table-name "$table" --select COUNT --query Count --output text)"
if [[ "$count" != "0" ]]; then
  echo "Tabela $table ma już $count wpisów, pomijam seed."
  exit 0
fi

# Rodzaj słownika w pliku → wartość klucza partycji.
items="$(jq -c '
  {players: "PLAYER", seasons: "SEASON", competitions: "COMPETITION", matches: "MATCH", sponsors: "SPONSOR"} as $kinds
  | to_entries[]
  | .key as $kind
  | .value[]
  | {PutRequest: {Item: {kind: {S: $kinds[$kind]}, id: {S: .id}, data: {S: tojson}}}}
' "$SEED")"
total="$(wc -l <<<"$items" | tr -d ' ')"

# BatchWriteItem przyjmuje do 25 zapisów naraz.
offset=0
while ((offset < total)); do
  batch="$(sed -n "$((offset + 1)),$((offset + 25))p" <<<"$items" | jq -sc --arg t "$table" '{($t): .}')"
  # shellcheck disable=SC2016 # `[]` to literał JMESPath, nie podstawienie powłoki
  unprocessed="$(aws dynamodb batch-write-item --request-items "$batch" --query 'length(UnprocessedItems.*[] || `[]`)' --output text)"
  if [[ "$unprocessed" != "0" ]]; then
    echo "DynamoDB nie przyjął $unprocessed zapisów; uruchom skrypt ponownie po wyczyszczeniu tabeli." >&2
    exit 1
  fi
  offset=$((offset + 25))
done
echo "Wgrano $total wpisów słowników do $table."
