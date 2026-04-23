#!/usr/bin/env bash
# Prompt for Discord credentials used by the TPP POC slash-command registration.
# Writes /tmp/wisp-creds/creds.env with mode 600.

set -euo pipefail

CRED_DIR="/tmp/wisp-creds"
CRED_FILE="$CRED_DIR/creds.env"

mkdir -p "$CRED_DIR"
chmod 700 "$CRED_DIR"

read -r -p "DISCORD_APPLICATION_ID: " APP_ID
read -r -s -p "DISCORD_BOT_TOKEN: " BOT_TOKEN
echo

umask 077
cat > "$CRED_FILE" <<EOF
DISCORD_APPLICATION_ID=$APP_ID
DISCORD_BOT_TOKEN=$BOT_TOKEN
EOF
chmod 600 "$CRED_FILE"

echo "Wrote $CRED_FILE (mode 600)"
