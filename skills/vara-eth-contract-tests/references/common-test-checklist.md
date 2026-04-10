# Common Test Checklist

Use this file to separate shared baseline checks from contract-specific or API-specific tests.

## Purpose

Before writing a new suite, decide whether the check already belongs to a shared baseline. If yes, reuse or extend the existing common test instead of rewriting it in every contract suite.

## Usually Shared Once

These checks are usually good candidates for one shared baseline suite or helper set:

- environment variables are present and have the expected shape
- shared clients initialize successfully
- provider connection works
- wallet signer is available
- router address is configured and readable
- base ETH balance is non-zero
- base WVARA balance or allowance preconditions are sane
- common helpers such as `wait1Block` or polling utilities behave as expected

## Usually Contract-Specific

These checks usually belong in the target contract suite:

- the chosen contract code id env var is present
- the program can be created for that contract
- the program appears on Vara-Eth
- executable balance top-up for that deployed program
- constructor or init flow
- contract metadata queries
- contract service function calls
- contract-specific state transitions
- injected transaction behavior for that contract

## Usually API-Specific

These checks belong in API-surface suites rather than contract suites:

- `InjectedTx` hash, message id, and signature derivation
- `InjectedTxPromise` hash and signature validation
- signature recovery
- router view methods
- mirror view methods
- `readState` and `readFullState` shape assertions
- queue, waitlist, stash, mailbox, and pages readers
- concurrency, nonce management, and mixed mirror/injected flows

## Decision Rule

When adding a new test, ask:

1. Would this same assertion be valid for most or all contracts in the repo?
2. Is it about shared infrastructure rather than the target contract's business behavior?
3. Would repeating it across suites make future maintenance noisier?

If the answer is mostly yes, put it in the shared baseline or shared helpers instead of duplicating it.

## Default Baseline For This Repo

With the current repo shape, prefer this split:

- shared baseline:
  env validation, connection setup, common balance sanity checks
- contract suite:
  deploy, top-up, init, metadata, messages, injected txs tied to that contract
- API suite:
  low-level object behavior, signatures, query readers, router and mirror views

## What The Skill Should Do

When using this skill on a future task:

- first scan for existing common tests under `test/`
- preserve and reuse those tests if they already cover the baseline
- avoid copying the same env/setup/balance assertions into every new contract suite
- only add a new shared baseline test when the assertion is truly cross-cutting
