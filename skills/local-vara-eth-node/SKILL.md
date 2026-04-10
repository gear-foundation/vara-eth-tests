---
name: local-vara-eth-node
description: Run a fully local Vara.eth development environment with ethexe, switch this repository to local RPC endpoints, fetch the local router address, and prepare `.env` for local integration-test debugging. Use when the task is to reproduce test failures locally, debug contract/program flows without testnet noise, or temporarily point the repo at a local ethexe dev node.
---

# Local Vara.eth Node

Use this skill when local debugging is needed for Vara.eth integration tests in this repository.

## Goal

Launch a fully local Vara.eth development environment using `ethexe`, with:

- local Anvil
- local Router deployment
- local Vara.eth RPC
- a usable Router address for CLI/tests
- `.env` updated to local endpoints when explicitly requested

## Clone Location

If the Gear repository is not already available locally for this task, clone it into:

- `/tmp/gear`

Do not clone it into this repository workspace.

## Recommended Flow

Use the built-in dev mode. It is the most reliable local setup in the current implementation.

## Foundry Version Requirement

For local `upload` / `requestCodeValidation` to work reliably with the current `ethexe` flow, use a compatible Foundry nightly.

Known working version:

```bash
foundryup --install nightly-c07d504b4ae67754584f4e05ff0c547a43c50f7b
```

Why this matters:

- local `ethexe` code upload uses the `requestCodeValidation(...)` path with `sidecar_7594`
- some stable Anvil / Foundry versions can fail with:
  - `error code -32602: Failed to decode transaction`
- if that happens, check `anvil --version` first and switch to the known working nightly above

## 1. Clone The Repository

```bash
git clone https://github.com/gear-tech/gear.git /tmp/gear
cd /tmp/gear/ethexe
```

## 2. Build `ethexe`

In practice, before the final Rust build you may need to prepare contract artifacts used by `ethexe-ethereum`.

### 2a. Initialize submodules

From `/tmp/gear`:

```bash
git submodule update --init --recursive
```

This is required because `ethexe/contracts/lib/*` submodules are needed for local contract ABI artifacts.

### 2b. Build contract artifacts

From `/tmp/gear/ethexe/contracts`:

```bash
forge build
```

This generates `out/*.json` artifacts used by `ethexe-ethereum`. If these artifacts are missing, the Rust build can fail with missing files like:

- `Vault.sol/Vault.json`
- `VaultFactory.sol/VaultFactory.json`
- `OperatorRegistry.sol/OperatorRegistry.json`

### 2c. Build the CLI binary

Build from the workspace root, not from `/tmp/gear/ethexe`:

```bash
cd /tmp/gear
cargo build --release -p ethexe-cli
```

The package name is `ethexe-cli`, while the resulting binary is `ethexe`.

Verify the binary starts:

```bash
/tmp/gear/target/release/ethexe --help
```

## 3. Start The Local Node

```bash
/tmp/gear/target/release/ethexe run \
  --dev \
  --tmp \
  --no-network \
  --rpc-port 9944
```

### Meaning of the flags

- `--dev`: start the local development environment automatically
- `--tmp`: use a temporary database for this run
- `--no-network`: disable P2P networking for a single-node local setup
- `--rpc-port 9944`: expose the Vara.eth RPC on port `9944`

## 4. Expected Local Endpoints

After startup, the local environment usually exposes:

- Ethereum RPC: `ws://127.0.0.1:8545`
- Vara.eth WS RPC: `ws://127.0.0.1:9944`
- Vara.eth HTTP RPC: `http://127.0.0.1:9944`

## 5. Get The Router Address

Do not hardcode the local Router address. Fetch it from the dev RPC:

```bash
curl http://127.0.0.1:9944 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"routerAddress","params":[],"id":1}'
```

Use the returned address in local CLI commands and test `.env` files.

## 6. Verify The Node Is Running

Check Ethereum RPC:

```bash
cast block-number --rpc-url http://127.0.0.1:8545
```

Check Vara.eth RPC:

```bash
curl http://127.0.0.1:9944 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"routerAddress","params":[],"id":1}'
```

## 7. Local Test Configuration

Typical local values:

```env
ETHEREUM_RPC=ws://127.0.0.1:8545
VARA_ETH_RPC=ws://127.0.0.1:9944
ROUTER_ADDRESS=<value from routerAddress>
PRIVATE_KEY=0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
```

The private key above is the default first Anvil account.

When switching this repository to local execution:

- update `.env` only for an explicit local-debugging flow
- clearly note that the repo is no longer pointed at testnet

## 8. Insert The Local Key Into `ethexe`

```bash
/tmp/gear/target/release/ethexe key insert 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
```

## 9. Optional: Debug Logs

For narrower runtime/program logs:

```bash
RUST_LOG="ethexe=info,gwasm=debug" /tmp/gear/target/release/ethexe run \
  --dev \
  --tmp \
  --no-network \
  --rpc-port 9944
```

To save logs to a file:

```bash
RUST_LOG="ethexe=info,gwasm=debug" /tmp/gear/target/release/ethexe run \
  --dev \
  --tmp \
  --no-network \
  --rpc-port 9944 2>&1 | tee /tmp/ethexe.log
```

The current local log file path used by this repo workflow is:

- `/tmp/ethexe.log`

## 10. Optional: Custom Block Time

```bash
/tmp/gear/target/release/ethexe run \
  --dev \
  --tmp \
  --no-network \
  --rpc-port 9944 \
  --block-time 6
```

Use this carefully. In the current dev flow, larger block times may affect local behavior and timing.

## 11. Stop The Node

Press `Ctrl+C`.

## 12. Reset State

If you used `--tmp`, stopping the node is usually enough.

If you used `--base <path>`, remove that directory's `db` subfolder to reset the node state.

## Common Pitfalls

- `ethexe tx` global flags must come before the subcommand.
- `ethexe tx query` requires `--rpc-url` for the Vara.eth WS RPC.
- `--dev` is the preferred local flow. Manual local stack assembly is more fragile.
- Do not assume a local Router address. Read it from `routerAddress`.
- If the Rust build fails with missing `symbiotic-*` or `Vault*.json` artifacts, run:
  - `git submodule update --init --recursive`
  - `forge build` inside `/tmp/gear/ethexe/contracts`
- The Rust package name is `ethexe-cli`, not `ethexe`.
- On the current local stack, `ethexe tx upload` may still fail with:
  - `error code -32602: Failed to decode transaction`
  This means the local node is up, but code validation/upload is still blocked at the Ethereum RPC layer.
- In practice, if local `upload` fails with `Failed to decode transaction`, a Foundry/Anvil version mismatch is a primary suspect.

## Example `tx query`

```bash
./target/release/ethexe tx \
  --ethereum-rpc "ws://127.0.0.1:8545" \
  --ethereum-router "$ROUTER_ADDRESS" \
  --sender "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266" \
  query \
  --rpc-url "ws://127.0.0.1:9944" \
  "$MIRROR_ADDRESS"
```
