# rust-sdk-tests

Rust integration tests for the Vara-Eth stack, colocated with the existing JS test harness.

## What It Uses

- the root `.env` from this repository
- remote testnet RPC endpoints, the same way the current `vara-eth-api` tests do
- git dependencies from `https://github.com/gear-tech/gear`

## Current Coverage

- router smoke checks
- mirror smoke checks for `TOKEN_ID`

## Run

```bash
cd rust-sdk-tests
cargo test
```

The test harness reads these values from the parent `.env`:

- `ETHEREUM_RPC`
- `VARA_ETH_RPC`
- `ROUTER_ADDRESS`
- `PRIVATE_KEY`
- `TOKEN_ID`

Environment variables override values from the `.env` file.
