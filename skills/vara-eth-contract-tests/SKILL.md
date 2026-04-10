---
name: vara-eth-contract-tests
description: Write or update Vara-Eth integration tests for a new contract or API surface in this repository. Use when the task is to add a Vitest suite under test/ for another contract, following the successful VFT pattern with shared setup from test/common.ts and test/config.ts, Sails IDL parsing, router deployment, executable balance top-up, handle queries, injected transactions, or lower-level vara-eth-api coverage for mirror, router, query, signer, and promise flows.
---

# Vara-Eth Contract Tests

Use this skill when asked to create or extend integration tests for a contract or a Vara-Eth API surface in this repo.

Treat [`test/vft/vft.test.ts`](/Users/luisa/vara-eth-tests/test/vft/vft.test.ts) as the primary reference. Do not copy patterns from the mandelbrot tests unless the user explicitly asks for them.

## Goal

Produce a Vitest integration suite that matches the repo's current testing style:

- shared runtime from [`test/common.ts`](/Users/luisa/vara-eth-tests/test/common.ts)
- env-driven configuration from [`test/config.ts`](/Users/luisa/vara-eth-tests/test/config.ts)
- sequential, stateful tests
- Sails-based payload encoding and reply decoding
- real deployment and message flow on Vara-Eth
- or direct `vara-eth-api` coverage of router, mirror, program query, injected tx, and signature flows when the task is API-first rather than contract-first

## First Reads

Before writing code, open only the files that matter:

1. [`test/common.ts`](/Users/luisa/vara-eth-tests/test/common.ts)
2. [`test/config.ts`](/Users/luisa/vara-eth-tests/test/config.ts)
3. [`test/vft/vft.test.ts`](/Users/luisa/vara-eth-tests/test/vft/vft.test.ts)
4. The target contract IDL under `target/wasm32-gear/release/*.idl`
5. The target contract wasm path only if needed to confirm naming
6. [`vitest.config.mts`](/Users/luisa/vara-eth-tests/vitest.config.mts) if file ordering matters
7. [`references/vara-eth-api-patterns.md`](/Users/luisa/vara-eth-tests/skills/vara-eth-contract-tests/references/vara-eth-api-patterns.md) when the request is about raw API behavior, signatures, promises, router views, or non-Sails payload testing
8. [`references/common-test-checklist.md`](/Users/luisa/vara-eth-tests/skills/vara-eth-contract-tests/references/common-test-checklist.md) to decide which baseline checks should already exist and should be reused instead of rewritten
9. [`references/reply-testing.md`](/Users/luisa/vara-eth-tests/skills/vara-eth-contract-tests/references/reply-testing.md) when the suite needs reply-code assertions, promise checks, or negative-case validation
10. [`references/environment-matrix.md`](/Users/luisa/vara-eth-tests/skills/vara-eth-contract-tests/references/environment-matrix.md) when deciding between local and testnet execution
11. [`references/code-id-and-env-rules.md`](/Users/luisa/vara-eth-tests/skills/vara-eth-contract-tests/references/code-id-and-env-rules.md) when the task involves `.env`, upload scripts, or switching networks
12. [`references/query-vs-message.md`](/Users/luisa/vara-eth-tests/skills/vara-eth-contract-tests/references/query-vs-message.md) when deciding whether a test should use query or real message flow
13. [`references/debug-playbook.md`](/Users/luisa/vara-eth-tests/skills/vara-eth-contract-tests/references/debug-playbook.md) when diagnosing reply failures, query failures, balance issues, or node-level traps

If the user names a contract, inspect its IDL and derive the suite from the actual constructors, services, functions, and queries instead of guessing.

## Repo Conventions

- First look for an existing shared test, helper, or baseline suite before adding a new copy of common checks.
- For contract-specific suites, create a dedicated folder under `test/<contract-name>/` instead of placing the file directly in `test/`.
- Prefer a file path like `test/<contract-name>/<contract-name>.test.ts` or `test/<contract-name>/<feature>.test.ts`.
- Reuse the globals exported from `test/common.ts` rather than creating a second setup layer.
- Keep top-level state in module variables when later tests depend on deployed program ids or state hashes.
- Use `Hex` from `viem` for ids and hashes.
- Read IDL with `readFileSync(..., "utf-8")`.
- Parse IDL with `sails.parseIdl(idlContent)` before using `ctors`, `services`, `functions`, or `queries`.
- Use `getMirrorClient(programId, walletClient, publicClient)` for executable balance top-ups and direct message sends.
- Use `varaEthApi.call.program.calculateReplyForHandle(...)` for read-style query checks.
- Keep assertions concrete: receipt status, reply code, decoded payload, balance change, metadata values.
- Distinguish carefully between receipt success and reply success; read `references/reply-testing.md` before asserting `replyCode` or injected promise `code`.
- When the task is about API behavior rather than contract semantics, prefer direct `createVaraEthApi(...)`, `InjectedTx`, `InjectedTxPromise`, router view methods, and raw payload assertions over Sails abstractions.

## Default Flow

For most new contracts, build the suite in this order:

1. Validate the env-provided code id.
2. Create the program through `ethereumClient.router.createProgram(codeId)`.
3. Wait until the program appears on Vara-Eth.
4. Approve `WVARA` for the deployed program.
5. Top up executable balance through the mirror client.
6. Read the resulting state and assert executable balance.
7. Parse IDL.
8. Send constructor or init message.
9. Assert metadata or initial state via query calls.
10. Test one or more mutating messages through `mirror.sendMessage(...)`.
11. If relevant, add injected transaction coverage through `varaEthApi.createInjectedTransaction(...)`.

Skip steps that do not apply to the contract, but preserve this overall style unless the user asks for a different shape.

## Choose The Right Test Style

Pick the narrowest style that matches the request:

1. Contract integration:
Use the `vft` pattern when testing a specific contract's constructor, service functions, queries, balances, and state changes through IDL-aware payloads.

2. API integration:
Use direct `vara-eth-api` patterns when testing `createVaraEthApi`, provider behavior, mirror/router view methods, raw payload sending, state readers, injected transactions, promise validation, or signature recovery.

3. Router and infrastructure:
Use router-centric tests when the request is about code validation, program creation, ABI-interface creation, validator metadata, or code state queries.

4. Signature and promise validation:
Use deterministic fixtures when the request is about hash derivation, message id derivation, signature creation, signature recovery, or promise validation. These tests should avoid unnecessary deployment if pure object-level coverage is enough.

## Naming And Env Rules

- Match the contract folder name to the contract/package name when possible, for example `test/vft/` or `test/mandelbrot-checker/`.
- Derive the env var name from the contract if one is not given:
  `TOKEN_ID`, `MANAGER_CODE_ID`, `<CONTRACT>_CODE_ID`, etc.
- Match the contract artifact names used under `target/wasm32-gear/release/`.
- Use clear describe blocks such as `create <contract>`, `metadata`, `send messages`, `injected txs`.

## Reliability Rules

- Prefer bounded waits over unbounded `while` loops when you introduce new polling logic.
- Reuse a helper like `waitForStateHashChange(...)` when state transitions matter.
- If a test depends on a previous test's state, keep both in the same file and preserve order-friendly structure.
- Avoid unnecessary imports, debug logs, or unused wasm byte loading unless the test truly needs them.
- Do not introduce mandelbrot-specific file artifacts or cross-file dependencies unless explicitly requested.
- Prefer bounded polling helpers or block waits over hot `while` loops.
- If a provider choice matters, mirror the intent of the scenario:
  `WsVaraEthProvider` for websocket/event-driven tests and `HttpVaraEthProvider` for HTTP query coverage.
- For API-first tests, assert object shapes and typed fields explicitly instead of only checking truthiness.
- Do not duplicate baseline setup tests if the repo already has them; extend or reference the shared baseline instead.

## Implementation Checklist

- Confirm the target contract name and artifact paths from `target/...`.
- Confirm the constructor or init entrypoint from the IDL.
- Confirm which query methods are safe and useful for assertions.
- Confirm which state-changing functions are worth covering first.
- Decide whether this suite is contract-first, API-first, router-first, or signature-first.
- Decide which parts belong to the shared baseline and which belong only to the target contract or API surface.
- Write the suite.
- If test ordering depends on this file, update [`vitest.config.mts`](/Users/luisa/vara-eth-tests/vitest.config.mts).
- Run the narrowest useful Vitest command when feasible.

## Reference

For reusable code shapes and example snippets, read:

- [`references/vft-patterns.md`](/Users/luisa/vara-eth-tests/skills/vara-eth-contract-tests/references/vft-patterns.md) for contract-first Sails-based suites
- [`references/vara-eth-api-patterns.md`](/Users/luisa/vara-eth-tests/skills/vara-eth-contract-tests/references/vara-eth-api-patterns.md) for raw API, router, mirror, query, injected tx, and signature coverage
- [`references/common-test-checklist.md`](/Users/luisa/vara-eth-tests/skills/vara-eth-contract-tests/references/common-test-checklist.md) for reusable baseline checks that should usually exist only once
- [`references/reply-testing.md`](/Users/luisa/vara-eth-tests/skills/vara-eth-contract-tests/references/reply-testing.md) for reply-code semantics, auto vs manual success, and negative-case test design
- [`references/environment-matrix.md`](/Users/luisa/vara-eth-tests/skills/vara-eth-contract-tests/references/environment-matrix.md) for deciding whether a scenario belongs on local or testnet
- [`references/code-id-and-env-rules.md`](/Users/luisa/vara-eth-tests/skills/vara-eth-contract-tests/references/code-id-and-env-rules.md) for `.env`, code id, and launcher rules
- [`references/query-vs-message.md`](/Users/luisa/vara-eth-tests/skills/vara-eth-contract-tests/references/query-vs-message.md) for execution-based query semantics versus real message flow
- [`references/debug-playbook.md`](/Users/luisa/vara-eth-tests/skills/vara-eth-contract-tests/references/debug-playbook.md) for common failure signatures and the next debugging step
