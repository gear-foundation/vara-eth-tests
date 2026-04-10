#!/bin/sh

set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)

usage() {
  cat <<'EOF'
Usage:
  ./script/run-tests.sh local rust
  ./script/run-tests.sh local ts
  TESTNET_PRIVATE_KEY=0x... TESTNET_SENDER=0x... ./script/run-tests.sh testnet rust
  TESTNET_PRIVATE_KEY=0x... TESTNET_SENDER=0x... ./script/run-tests.sh testnet ts

Notes:
  - local: switches .env to local RPCs and uploads fresh local code ids before tests
  - testnet: switches .env to testnet RPCs; TESTNET_PRIVATE_KEY is required
  - if TESTNET_SENDER is omitted, the script will try to derive it via cast
EOF
}

if [ "$#" -ne 2 ]; then
  usage >&2
  exit 1
fi

NETWORK="$1"
SUITE="$2"

derive_sender_from_private_key() {
  private_key="$1"

  if ! command -v cast >/dev/null 2>&1; then
    return 1
  fi

  cast wallet address --private-key "$private_key" 2>/dev/null
}

build_local_wasm() {
  echo
  echo "Building local wasm artifacts"
  (
    cd "$REPO_ROOT"
    cargo build --release
  )
}

run_rust_tests() {
  echo
  echo "Running Rust SDK tests"
  (
    cd "$REPO_ROOT/rust-sdk-tests"
    cargo test -- --nocapture
  )
}

run_ts_tests() {
  echo
  echo "Running TypeScript tests"
  (
    cd "$REPO_ROOT"
    pnpm exec vitest run
  )
}

case "$NETWORK" in
  local)
    echo "Preparing local environment"
    "$REPO_ROOT/script/use-local-env.sh"

    build_local_wasm

    echo
    echo "Uploading local code ids"
    "$REPO_ROOT/script/upload-code-ids.sh"
    ;;
  testnet)
    : "${TESTNET_PRIVATE_KEY:?TESTNET_PRIVATE_KEY is required for testnet runs}"

    if [ -z "${TESTNET_SENDER:-}" ]; then
      TESTNET_SENDER=$(derive_sender_from_private_key "$TESTNET_PRIVATE_KEY" || true)
    fi

    : "${TESTNET_SENDER:?TESTNET_SENDER is required for testnet runs (or install cast so it can be derived automatically)}"

    echo "Preparing testnet environment"
    TESTNET_PRIVATE_KEY="$TESTNET_PRIVATE_KEY" \
      TESTNET_SENDER="$TESTNET_SENDER" \
      "$REPO_ROOT/script/use-testnet-env.sh"
    ;;
  *)
    echo "Unknown network: $NETWORK" >&2
    usage >&2
    exit 1
    ;;
esac

case "$SUITE" in
  rust)
    run_rust_tests
    ;;
  ts)
    run_ts_tests
    ;;
  *)
    echo "Unknown suite: $SUITE" >&2
    usage >&2
    exit 1
    ;;
esac
