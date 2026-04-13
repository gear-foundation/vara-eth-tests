#!/bin/sh

set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)

ENV_FILE="${ENV_FILE:-$REPO_ROOT/.env}"
TESTNET_ETHEREUM_RPC="${TESTNET_ETHEREUM_RPC:-wss://hoodi-reth-rpc.gear-tech.io/ws}"
TESTNET_VARA_ETH_RPC="${TESTNET_VARA_ETH_RPC:-wss://vara-eth-validator-1.gear-tech.io}"
TESTNET_ROUTER_ADDRESS="${TESTNET_ROUTER_ADDRESS:-0xE549b0AfEdA978271FF7E712232B9F7f39A0b060}"

TESTNET_PRIVATE_KEY="${TESTNET_PRIVATE_KEY:-}"
TESTNET_SENDER="${TESTNET_SENDER:-}"

if [ -z "$TESTNET_PRIVATE_KEY" ]; then
  echo "TESTNET_PRIVATE_KEY is required" >&2
  exit 1
fi

if [ -z "$TESTNET_SENDER" ]; then
  echo "TESTNET_SENDER is required" >&2
  exit 1
fi

if [ ! -f "$ENV_FILE" ]; then
  touch "$ENV_FILE"
  echo "Created env file: $ENV_FILE"
fi

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

echo "Switching $ENV_FILE to hardcoded testnet values"

update_env_value "ETHEREUM_RPC" "$TESTNET_ETHEREUM_RPC"
update_env_value "VARA_ETH_RPC" "$TESTNET_VARA_ETH_RPC"
update_env_value "ROUTER_ADDRESS" "$TESTNET_ROUTER_ADDRESS"
update_env_value "PRIVATE_KEY" "$TESTNET_PRIVATE_KEY"
update_env_value "SENDER" "$TESTNET_SENDER"

echo
echo "Done. .env now points to testnet RPC endpoints."
echo "CHECKER_CODE_ID / MANAGER_CODE_ID / TOKEN_ID / PROGRAM_COUNT were left unchanged."
