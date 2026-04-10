# Testing Workflow Report

This document summarizes the testing-related documentation, skills, and scripts that were added or updated in this repository.

## Goal

The work focused on making the test workflow more repeatable across:

- local Vara-Eth node runs
- testnet runs
- Rust SDK integration tests
- TypeScript / Vitest integration tests

It also captured the rules for checking upstream dependency changes before changing tests.

## AGENTS.md

[`AGENTS.md`](AGENTS.md) now captures the stable workflow rules.

### What it covers

- dependency review workflow for TypeScript tests
- dependency review workflow for Rust SDK tests
- how to analyze `@vara-eth/api` releases
- how to analyze `ethexe` / Gear releases relevant to the Rust SDK tests
- release comparison rule
- release drift rule
- local debugging entrypoint through the `local-vara-eth-node` skill
- test launchers for local and testnet runs
- canonical rule for syncing local code ids into `.env`

### Why it exists

The intent is to keep test changes tied to real upstream API or SDK behavior, rather than patching tests blindly when something fails.

## Skills

Two repo skills are now part of the testing workflow.

### `local-vara-eth-node`

File:

- [`skills/local-vara-eth-node/SKILL.md`](skills/local-vara-eth-node/SKILL.md)

Purpose:

- start a fully local `ethexe` node
- understand local RPC endpoints
- fetch the router address
- use the default local Anvil key
- record logs to a file
- document the known-good local launch flow

### `vara-eth-contract-tests`

File:

- [`skills/vara-eth-contract-tests/SKILL.md`](skills/vara-eth-contract-tests/SKILL.md)

Purpose:

- write or update Vara-Eth integration tests
- follow the repo's VFT-based testing style
- choose between contract-first, API-first, and router-first tests
- keep tests aligned with current repo patterns

## References Added To `vara-eth-contract-tests`

The skill now points to a wider set of focused reference files.

### Existing references

- [`skills/vara-eth-contract-tests/references/vft-patterns.md`](skills/vara-eth-contract-tests/references/vft-patterns.md)
- [`skills/vara-eth-contract-tests/references/vara-eth-api-patterns.md`](skills/vara-eth-contract-tests/references/vara-eth-api-patterns.md)
- [`skills/vara-eth-contract-tests/references/common-test-checklist.md`](skills/vara-eth-contract-tests/references/common-test-checklist.md)
- [`skills/vara-eth-contract-tests/references/reply-testing.md`](skills/vara-eth-contract-tests/references/reply-testing.md)

### New references

- [`skills/vara-eth-contract-tests/references/environment-matrix.md`](skills/vara-eth-contract-tests/references/environment-matrix.md)
  - when to use local vs testnet
  - how to interpret local vs testnet failures

- [`skills/vara-eth-contract-tests/references/code-id-and-env-rules.md`](skills/vara-eth-contract-tests/references/code-id-and-env-rules.md)
  - `.env` switching rules
  - code id update rules
  - launcher and upload responsibilities

- [`skills/vara-eth-contract-tests/references/query-vs-message.md`](skills/vara-eth-contract-tests/references/query-vs-message.md)
  - execution-based query vs real message flow
  - when query is appropriate
  - when message-based checks are safer

- [`skills/vara-eth-contract-tests/references/debug-playbook.md`](skills/vara-eth-contract-tests/references/debug-playbook.md)
  - common failure signatures
  - likely causes
  - recommended next debugging step

## Scripts Added Or Updated

### Test launchers

- [`script/run-tests.sh`](script/run-tests.sh)

Provides a single user-facing entrypoint:

- `./script/run-tests.sh local rust`
- `./script/run-tests.sh local ts`
- `TESTNET_PRIVATE_KEY=0x... TESTNET_SENDER=0x... ./script/run-tests.sh testnet rust`
- `TESTNET_PRIVATE_KEY=0x... TESTNET_SENDER=0x... ./script/run-tests.sh testnet ts`

Behavior:

- local:
  - switches `.env` to local RPC values
  - builds local wasm artifacts
  - uploads local code ids
  - runs the selected test suite
- testnet:
  - requires an explicit private key
  - switches `.env` to testnet RPC values
  - runs the selected test suite

### Environment switchers

- [`script/use-local-env.sh`](script/use-local-env.sh)
- [`script/use-testnet-env.sh`](script/use-testnet-env.sh)

Rules:

- local switcher updates local RPC/router/key/sender values
- testnet switcher updates only testnet RPC/router and optional key/sender
- testnet switcher must not overwrite code ids

### Code id synchronization

- [`script/upload-code-ids.sh`](script/upload-code-ids.sh)

Role:

- upload local wasm code through `ethexe`
- sync the following values in the root `.env`:
  - `TOKEN_ID`
  - `CHECKER_CODE_ID`
  - `MANAGER_CODE_ID`

It also tolerates already-uploaded / already-validated cases and continues where appropriate.

## Practical Workflow

### Local

Preferred entrypoint:

```bash
./script/run-tests.sh local rust
./script/run-tests.sh local ts
```

This is the recommended path for:

- fast iteration
- contract debugging
- payload debugging
- log-heavy diagnosis
- rebuilding and re-uploading code ids

### Testnet

Preferred entrypoint:

```bash
TESTNET_PRIVATE_KEY=0x... TESTNET_SENDER=0x... ./script/run-tests.sh testnet rust
TESTNET_PRIVATE_KEY=0x... TESTNET_SENDER=0x... ./script/run-tests.sh testnet ts
```

This is the recommended path for:

- validating behavior against remote infrastructure
- checking testnet-specific timing or transport behavior
- confirming that local fixes still hold on a real deployment target

## Key Testing Insights Captured In Docs

The documentation now reflects several important testing lessons:

- before patching tests, inspect the relevant upstream release or SDK implementation
- local and testnet should be treated as different environments, not interchangeable copies
- execution-based query behavior should not be confused with raw state reading
- `.env` switching and code id syncing should be handled by dedicated scripts instead of manual edits
- the launch flow should be predictable enough that a user can run tests through a single command

## Scope Of This Report

This report is intentionally about the testing workflow documentation and tooling, not about every individual contract bug or temporary debugging experiment.

Stable workflow and operational guidance belong in:

- [`AGENTS.md`](AGENTS.md)
- [`skills/local-vara-eth-node/SKILL.md`](skills/local-vara-eth-node/SKILL.md)
- [`skills/vara-eth-contract-tests/SKILL.md`](skills/vara-eth-contract-tests/SKILL.md)
- the references under [`skills/vara-eth-contract-tests/references/`](skills/vara-eth-contract-tests/references/)
