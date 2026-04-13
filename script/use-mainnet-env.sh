#!/bin/sh

set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)

ENV_FILE="${ENV_FILE:-$REPO_ROOT/.env}"
MAINNET_ETHEREUM_RPC="${MAINNET_ETHEREUM_RPC:-wss://mainnet-reth-rpc.gear-tech.io/ws}"
MAINNET_VARA_ETH_RPC="${MAINNET_VARA_ETH_RPC:-wss://validator-1-eth.vara.network}"
MAINNET_ROUTER_ADDRESS="${MAINNET_ROUTER_ADDRESS:-0x9C13FE9242dfe2ba2Cd446480A9308279aA74cb6}"

MAINNET_PRIVATE_KEY="${MAINNET_PRIVATE_KEY:-}"
MAINNET_SENDER="${MAINNET_SENDER:-}"

if [ -z "$MAINNET_PRIVATE_KEY" ]; then
  echo "MAINNET_PRIVATE_KEY is required" >&2
  exit 1
fi

if [ -z "$MAINNET_SENDER" ]; then
  echo "MAINNET_SENDER is required" >&2
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

echo "Switching $ENV_FILE to hardcoded mainnet values"

update_env_value "ETHEREUM_RPC" "$MAINNET_ETHEREUM_RPC"
update_env_value "VARA_ETH_RPC" "$MAINNET_VARA_ETH_RPC"
update_env_value "ROUTER_ADDRESS" "$MAINNET_ROUTER_ADDRESS"
update_env_value "PRIVATE_KEY" "$MAINNET_PRIVATE_KEY"
update_env_value "SENDER" "$MAINNET_SENDER"

echo
echo "Done. .env now points to mainnet RPC endpoints."
echo "CHECKER_CODE_ID / MANAGER_CODE_ID / TOKEN_ID / PROGRAM_COUNT were left unchanged."
