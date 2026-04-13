#!/bin/sh

set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)

ENV_FILE="${ENV_FILE:-$REPO_ROOT/.env}"

LOCAL_ETHEREUM_RPC="${LOCAL_ETHEREUM_RPC:-ws://127.0.0.1:8545}"
LOCAL_VARA_ETH_RPC="${LOCAL_VARA_ETH_RPC:-ws://127.0.0.1:9944}"
LOCAL_ROUTER_ADDRESS="${LOCAL_ROUTER_ADDRESS:-0xcf7ed3acca5a467e9e704c703e8d87f634fb0fc9}"
LOCAL_PRIVATE_KEY="${LOCAL_PRIVATE_KEY:-0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80}"
LOCAL_SENDER="${LOCAL_SENDER:-0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266}"

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

echo "Switching $ENV_FILE to local Vara.eth values"

update_env_value "ETHEREUM_RPC" "$LOCAL_ETHEREUM_RPC"
update_env_value "VARA_ETH_RPC" "$LOCAL_VARA_ETH_RPC"
update_env_value "ROUTER_ADDRESS" "$LOCAL_ROUTER_ADDRESS"
update_env_value "PRIVATE_KEY" "$LOCAL_PRIVATE_KEY"
update_env_value "SENDER" "$LOCAL_SENDER"

echo
echo "Done. .env now points to local RPC endpoints."
echo "CHECKER_CODE_ID / MANAGER_CODE_ID / TOKEN_ID / PROGRAM_COUNT were left unchanged."
