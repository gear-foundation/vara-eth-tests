# Debug Playbook

Use this file when a Rust or TypeScript Vara-Eth test fails and you need the next debugging step quickly.

## Start Narrow

Before widening the run:

1. reproduce with the smallest relevant test
2. prefer one contract or one scenario
3. use `--nocapture` or the narrowest `vitest` file when logs matter

## Common Symptoms

### Reply code `[1, 0, 0, 0]`

Meaning:

- execution ran out of gas in the Vara-side path

Check next:

- node logs
- executable balance of the program
- whether the failure is on query or on real message execution

### `custom error 0x2628d198`

Usually treat this as:

- code already uploaded or already validated

Check next:

- whether the code id was already present on that network
- whether `.env` already contains the expected code id

### `nonce too low`

Usually means:

- mixed nonce management
- stale cached sender state
- raw transactions colliding with SDK-managed transactions

Check next:

- pending nonce
- latest nonce
- whether raw and SDK flows are mixed in the same phase

### `The background task closed connection closed`

Usually means:

- websocket transport died
- provider/client needs reconnection

Check next:

- RPC endpoint health
- whether the test holds a client for too long during heavy activity
- whether the test should recreate clients after raw nonce-managed phases

### Traps about program data / allocations / wasm instantiation

Examples:

- `Not enough gas to handle program data`
- `Not enough gas to obtain program allocations`
- `Not enough gas to instantiate Function section of Wasm module`

Interpretation:

- suspect query/runtime preparation path first
- do not assume the cheap getter itself is the root cause

Check next:

- whether the failing path is query-based
- current executable balance
- state growth size
- node debug logs

## Executable Balance Rule

If behavior becomes strange under stress:

1. read `executable_balance` for the relevant programs
2. compare before and after the failing stage
3. do not assume every weird reply code is a pure logic bug

Insufficient executable balance can masquerade as runtime/query instability.

## Node Logs

For local debugging, prefer a node launch with useful runtime logs enabled.

Typical useful categories:

- `ethexe`
- `ethexe_rpc`
- `ethexe_processor`
- `ethexe_runtime_common`
- `ethexe_runtime`
- `gwasm`

Then search the log for:

- `out of gas`
- `trap`
- `reply`
- `allocations`
- `gas`
- `panic`

## Decision Rule

When a failure appears in a getter or query assertion:

1. ask whether the path is query-based or message-based
2. inspect executable balance
3. inspect node logs
4. only then decide whether the test, the contract, or the environment is the primary suspect
