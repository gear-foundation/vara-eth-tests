#!/bin/sh

set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)

ENV_FILE="${ENV_FILE:-$REPO_ROOT/.env}"
ETHEXE_BIN="${ETHEXE_BIN:-/tmp/gear/target/release/ethexe}"

if [ ! -f "$ENV_FILE" ]; then
  echo "Missing .env file: $ENV_FILE" >&2
  exit 1
fi

if [ ! -x "$ETHEXE_BIN" ]; then
  echo "ethexe binary is not executable: $ETHEXE_BIN" >&2
  exit 1
fi

set -a
. "$ENV_FILE"
set +a

: "${ETHEREUM_RPC:?ETHEREUM_RPC is required in .env}"
: "${SENDER:?SENDER is required in .env}"
: "${ROUTER_ADDRESS:?ROUTER_ADDRESS is required in .env}"

VFT_WASM="${VFT_WASM:-$REPO_ROOT/target/wasm32-gear/release/extended_vft.opt.wasm}"
CHECKER_WASM="${CHECKER_WASM:-$REPO_ROOT/target/wasm32-gear/release/mandelbrot_checker.opt.wasm}"
MANAGER_WASM="${MANAGER_WASM:-$REPO_ROOT/target/wasm32-gear/release/manager.opt.wasm}"

detect_router_address() {
  if ! command -v curl >/dev/null 2>&1; then
    return 1
  fi

  case "${VARA_ETH_RPC:-}" in
    ws://*)
      rpc_http_url="http://${VARA_ETH_RPC#ws://}"
      ;;
    wss://*)
      rpc_http_url="https://${VARA_ETH_RPC#wss://}"
      ;;
    http://*|https://*)
      rpc_http_url="$VARA_ETH_RPC"
      ;;
    *)
      return 1
      ;;
  esac

  response=$(curl -s "$rpc_http_url" \
    -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","method":"routerAddress","params":[],"id":1}') || return 1

  detected_router=$(printf '%s' "$response" | sed -n 's/.*"result":"\([^"]*\)".*/\1/p')
  [ -n "$detected_router" ] || return 1

  printf '%s' "$detected_router"
}

CURRENT_ROUTER_ADDRESS=$(detect_router_address || true)
if [ -n "$CURRENT_ROUTER_ADDRESS" ]; then
  ROUTER_ADDRESS="$CURRENT_ROUTER_ADDRESS"
fi

FAILED_UPLOADS=""

env_key_for_label() {
  case "$1" in
    extended_vft)
      printf '%s' "TOKEN_ID"
      ;;
    mandelbrot_checker)
      printf '%s' "CHECKER_CODE_ID"
      ;;
    manager)
      printf '%s' "MANAGER_CODE_ID"
      ;;
    *)
      return 1
      ;;
  esac
}

extract_code_id() {
  printf '%s\n' "$1" | sed -n 's/^[[:space:]]*Code id:[[:space:]]*\(0x[0-9a-fA-F]\{64\}\).*/\1/p' | tail -n 1
}

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
  echo "Updated $env_key in $ENV_FILE -> $env_value"
}

upload_code() {
  label="$1"
  wasm_path="$2"

  if [ ! -f "$wasm_path" ]; then
    echo "Missing wasm for $label: $wasm_path" >&2
    return 1
  fi

  echo
  echo "== Uploading $label =="
  echo "WASM: $wasm_path"

  set +e
  upload_output=$("$ETHEXE_BIN" --cfg none tx \
    --ethereum-rpc "$ETHEREUM_RPC" \
    --ethereum-router "$ROUTER_ADDRESS" \
    --sender "$SENDER" \
    --eip1559-fee-increase-percentage 0 \
    --blob-gas-multiplier 1 \
    upload "$wasm_path" -w 2>&1)
  upload_status=$?
  set -e

  printf '%s\n' "$upload_output"

  code_id=$(extract_code_id "$upload_output" || true)
  if [ -n "${code_id:-}" ]; then
    env_key=$(env_key_for_label "$label" || true)
    if [ -n "${env_key:-}" ]; then
      update_env_value "$env_key" "$code_id"
    fi
  fi

  if [ "$upload_status" -eq 0 ]; then
    return 0
  fi

  case "$upload_output" in
    *CodeAlreadyOnValidationOrValidated*|*already\ validated*|*already\ uploaded*|*custom\ error\ 0x2628d198*)
      echo "$label is already uploaded or validated, continuing"
      return 0
      ;;
  esac

  echo "Upload failed for $label, continuing" >&2
  return 1
}

echo "Using ethexe: $ETHEXE_BIN"
echo "Using Ethereum RPC: $ETHEREUM_RPC"
echo "Using Router: $ROUTER_ADDRESS"
echo "Using Sender: $SENDER"

if ! upload_code "extended_vft" "$VFT_WASM"; then
  FAILED_UPLOADS="$FAILED_UPLOADS extended_vft"
fi

if ! upload_code "mandelbrot_checker" "$CHECKER_WASM"; then
  FAILED_UPLOADS="$FAILED_UPLOADS mandelbrot_checker"
fi

if ! upload_code "manager" "$MANAGER_WASM"; then
  FAILED_UPLOADS="$FAILED_UPLOADS manager"
fi

if [ -n "$FAILED_UPLOADS" ]; then
  echo
  echo "Completed with upload errors:$FAILED_UPLOADS" >&2
  exit 1
fi

echo
echo "All uploads completed successfully"
