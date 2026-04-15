#!/bin/sh

set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
TEST_FILE="${VFT_DEBUG_TEST_FILE:-test/vft/vft.test.ts}"

: "${TESTNET_PRIVATE_KEY:?TESTNET_PRIVATE_KEY is required}"
: "${TESTNET_SENDER:?TESTNET_SENDER is required}"
: "${TESTNET_TOKEN_ID:?TESTNET_TOKEN_ID is required}"

case "$TEST_FILE" in
  test/vft/vft.test.ts|test/vft/vft.load.test.ts)
    ;;
  *)
    echo "Unsupported VFT debug test file: $TEST_FILE" >&2
    exit 1
    ;;
esac

case "${VFT_INJECTED_VALIDATOR_MODE:-default}" in
  default|slot)
    ;;
  *)
    echo "Unsupported VFT_INJECTED_VALIDATOR_MODE: ${VFT_INJECTED_VALIDATOR_MODE}" >&2
    exit 1
    ;;
esac

echo "Preparing testnet environment for manual VFT debug suite"
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
  echo "Updated $env_key for manual VFT debug run"
}

update_env_value "TOKEN_ID" "$TESTNET_TOKEN_ID"

echo
echo "Running manual VFT debug suite"
echo "Test file: $TEST_FILE"
echo "Injected validator mode: ${VFT_INJECTED_VALIDATOR_MODE:-default}"
(
  cd "$REPO_ROOT"
  VFT_INJECTED_VALIDATOR_MODE="${VFT_INJECTED_VALIDATOR_MODE:-default}" \
    pnpm exec vitest run "$TEST_FILE"
)

echo
echo "Manual VFT debug suite finished successfully."
