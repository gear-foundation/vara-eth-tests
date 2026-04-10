# Code Id And Env Rules

Use this file when the task involves switching between local and testnet, uploading wasm code, or interpreting `.env` values.

## Core Rule

Treat RPC endpoints and code ids as different layers.

- RPC and router values describe the target environment
- code ids describe uploaded wasm in that environment

Do not mix local RPC values with stale testnet code ids or the other way around.

## Canonical Scripts

Use these scripts as the default workflow:

- `./script/use-local-env.sh`
- `./script/use-testnet-env.sh`
- `./script/upload-code-ids.sh`
- `./script/run-tests.sh ...`

## Local Rules

For local runs:

- `.env` should point to local RPC endpoints
- wasm artifacts should be rebuilt locally
- code ids should be refreshed from local uploads

Canonical flow:

1. `./script/use-local-env.sh`
2. build local wasm artifacts
3. `./script/upload-code-ids.sh`
4. run tests

The launcher already does this:

- `./script/run-tests.sh local rust`
- `./script/run-tests.sh local ts`

## Testnet Rules

For testnet runs:

- `.env` should point to testnet RPC endpoints
- testnet runs require an explicit private key
- testnet env switching must not overwrite local/test-specific code ids automatically unless the task explicitly requires it

Use:

- `TESTNET_PRIVATE_KEY=0x... TESTNET_SENDER=0x... ./script/use-testnet-env.sh`

The testnet launcher wraps this:

- `TESTNET_PRIVATE_KEY=0x... TESTNET_SENDER=0x... ./script/run-tests.sh testnet rust`
- `TESTNET_PRIVATE_KEY=0x... TESTNET_SENDER=0x... ./script/run-tests.sh testnet ts`

## What Should Update Code Ids

`./script/upload-code-ids.sh` is the canonical way to refresh:

- `TOKEN_ID`
- `CHECKER_CODE_ID`
- `MANAGER_CODE_ID`

in the root `.env`.

`use-testnet-env.sh` should only switch:

- `ETHEREUM_RPC`
- `VARA_ETH_RPC`
- `ROUTER_ADDRESS`
- and optional `PRIVATE_KEY` / `SENDER`

It should not rewrite code ids.

## Practical Rule

When a test suddenly deploys the wrong contract or fails to create/query a program:

1. check the current `.env`
2. check which network it points to
3. check whether code ids came from the same environment
4. if local, refresh uploads
5. if testnet, confirm the code ids are the intended deployed ones
