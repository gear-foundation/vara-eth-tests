#!/bin/sh

set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)

ENV_FILE="${ENV_FILE:-$REPO_ROOT/.env}"
TEMPLATE_FILE="${TEMPLATE_FILE:-$REPO_ROOT/.env.example}"

if [ ! -f "$ENV_FILE" ]; then
  echo "Missing env file: $ENV_FILE" >&2
  exit 1
fi

if [ ! -f "$TEMPLATE_FILE" ]; then
  echo "Missing template file: $TEMPLATE_FILE" >&2
  exit 1
fi

set -a
. "$TEMPLATE_FILE"
set +a

: "${ETHEREUM_RPC:?ETHEREUM_RPC is required in $TEMPLATE_FILE}"
: "${VARA_ETH_RPC:?VARA_ETH_RPC is required in $TEMPLATE_FILE}"
: "${ROUTER_ADDRESS:?ROUTER_ADDRESS is required in $TEMPLATE_FILE}"

TESTNET_PRIVATE_KEY="${TESTNET_PRIVATE_KEY:-}"
TESTNET_SENDER="${TESTNET_SENDER:-}"

update_env_value() {
  env_key="$1"
  env_value="$2"
  tmp_file=$(mktemp "${ENV_FILE}.tmp.XXXXXX")

  awk -v key="$env_key" -v value="$env_value" '
    BEGIN { updated = 0 }
    index($0, key "=") == 1 {
      print key "=" value
      updated = 1
      next
    }
    { print }
    END {
      if (!updated) {
        print key "=" value
      }
    }
  ' "$ENV_FILE" > "$tmp_file"

  mv "$tmp_file" "$ENV_FILE"
  echo "Updated $env_key -> $env_value"
}

echo "Switching $ENV_FILE to testnet values from $TEMPLATE_FILE"

update_env_value "ETHEREUM_RPC" "$ETHEREUM_RPC"
update_env_value "VARA_ETH_RPC" "$VARA_ETH_RPC"
update_env_value "ROUTER_ADDRESS" "$ROUTER_ADDRESS"

if [ -n "$TESTNET_PRIVATE_KEY" ]; then
  update_env_value "PRIVATE_KEY" "$TESTNET_PRIVATE_KEY"
else
  echo "PRIVATE_KEY was not updated. Pass TESTNET_PRIVATE_KEY=... if needed."
fi

if [ -n "$TESTNET_SENDER" ]; then
  update_env_value "SENDER" "$TESTNET_SENDER"
else
  echo "SENDER was not updated. Pass TESTNET_SENDER=... if needed."
fi

echo
echo "Done. .env now points to testnet RPC endpoints."
echo "CHECKER_CODE_ID / MANAGER_CODE_ID / TOKEN_ID / PROGRAM_COUNT were left unchanged."
