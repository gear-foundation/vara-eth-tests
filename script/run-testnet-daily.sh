#!/bin/sh

set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)

: "${TESTNET_PRIVATE_KEY:?TESTNET_PRIVATE_KEY is required}"
: "${TESTNET_SENDER:?TESTNET_SENDER is required}"

echo "Preparing testnet environment"
TESTNET_PRIVATE_KEY="$TESTNET_PRIVATE_KEY" \
TESTNET_SENDER="$TESTNET_SENDER" \
  "$REPO_ROOT/script/use-testnet-env.sh"

echo
echo "Running Rust testnet daily suite"
(
  cd "$REPO_ROOT"
  cargo test -p rust-sdk-tests --test mandelbrot_checker mandelbrot_checker_profile_on_testnet -- --nocapture
)

echo
echo "Running TypeScript testnet daily suite"
(
  cd "$REPO_ROOT"
  pnpm exec vitest run test/setup.test.ts test/balance.test.ts test/vft/vft.load.test.ts
)

echo
echo "Testnet daily suite finished successfully."
