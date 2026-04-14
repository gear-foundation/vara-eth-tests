#!/bin/sh

set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)

: "${TESTNET_PRIVATE_KEY:?TESTNET_PRIVATE_KEY is required}"
: "${TESTNET_SENDER:?TESTNET_SENDER is required}"
: "${TESTNET_TOKEN_ID:?TESTNET_TOKEN_ID is required}"

echo "Preparing testnet environment for TypeScript daily suite"
TESTNET_PRIVATE_KEY="$TESTNET_PRIVATE_KEY" \
TESTNET_SENDER="$TESTNET_SENDER" \
  "$REPO_ROOT/script/use-testnet-env.sh"

update_env_value() {
  env_key="$1"
  env_value="$2"
  tmp_file=$(mktemp "$REPO_ROOT/.env.tmp.XXXXXX")

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
  ' "$REPO_ROOT/.env" > "$tmp_file"

  mv "$tmp_file" "$REPO_ROOT/.env"
  echo "Updated $env_key for testnet TypeScript daily run"
}

update_env_value "TOKEN_ID" "$TESTNET_TOKEN_ID"

echo
echo "Running TypeScript testnet daily suite"
(
  cd "$REPO_ROOT"
  pnpm exec vitest run test/setup.test.ts test/balance.test.ts test/vft/vft.test.ts
)

echo
echo "TypeScript testnet daily suite finished successfully."
