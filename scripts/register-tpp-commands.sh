#!/usr/bin/env bash
# Re-register /tpp-setup (Stage 2 schema: no url option) and /tpp-ping.
# Discord's POST /applications/{app_id}/commands with an existing command
# name overwrites that command, so running this is safe even if the commands
# are already registered under the old schema.
#
# Prereq: /tmp/wisp-creds/creds.env written by input-tpp-creds.sh

set -euo pipefail

CRED_FILE="/tmp/wisp-creds/creds.env"
test -f "$CRED_FILE" || { echo "creds file missing: run input-tpp-creds.sh first"; exit 1; }

set -a; source "$CRED_FILE"; set +a
: "${DISCORD_APPLICATION_ID:?}"
: "${DISCORD_BOT_TOKEN:?}"

URL="https://discord.com/api/v10/applications/$DISCORD_APPLICATION_ID/commands"
AUTH="Authorization: Bot $DISCORD_BOT_TOKEN"
CT="Content-Type: application/json"

redact() { sed -E 's/"(token|bot_token)":"[^"]*"/"\1":"REDACTED"/g'; }

echo "=== Registering /tpp-setup (Stage 2: no options) ==="
curl -sS -w "\nHTTP %{http_code}\n" -X POST "$URL" -H "$AUTH" -H "$CT" -d '{
  "name": "tpp-setup",
  "description": "Authorize Wisp to post to a Discord channel via webhook",
  "integration_types": [1],
  "contexts": [0, 1, 2]
}' | redact

echo
echo "=== Registering /tpp-ping ==="
curl -sS -w "\nHTTP %{http_code}\n" -X POST "$URL" -H "$AUTH" -H "$CT" -d '{
  "name": "tpp-ping",
  "description": "Send a test button message to the registered webhook",
  "integration_types": [1],
  "contexts": [0, 1, 2]
}' | redact
