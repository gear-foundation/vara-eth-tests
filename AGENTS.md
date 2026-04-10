# AGENTS

## Dependency Review Workflow

When changing or validating tests in this repository, first check whether the underlying Vara-Eth client or SDK behavior changed upstream.

### TypeScript tests

Files:
- `test/**/*.test.ts`
- `test/common.ts`
- `test/helpers/**/*.ts`

Source of truth:
- published `@vara-eth/api` releases on GitHub/npm
- published `@vara-eth/viem` / `viem` compatibility used by that release

Release pages:
- GitHub releases: `https://github.com/gear-tech/gear-js/releases`
- Repository: `https://github.com/gear-tech/gear-js`

Workflow:
1. Read the currently used versions from `package.json` and `pnpm-lock.yaml`.
2. Look specifically at releases relevant to `@vara-eth/api`, not at unrelated repository-wide changes in `gear-tech/gear-js`.
3. Compare the current project version with the relevant `@vara-eth/api` release/tag.
4. Review release notes / changelog / tagged diff for changes in `@vara-eth/api`.
5. Decide whether tests need updates:
   - update tests if public API, reply-code handling, injected tx behavior, mirror/router helpers, or connection semantics changed
   - do not change tests if the release only contains unrelated internal changes
6. After updating TS dependencies, re-run TypeScript-oriented checks first:
   - `pnpm exec tsc --noEmit`
   - then the relevant Vitest files

Notes:
- Prefer published releases over local unpublished code when deciding how TS tests should behave.
- If runtime behavior differs from older tests, prefer adjusting the assertions to the current published API semantics rather than preserving outdated assumptions.
- When given a GitHub repository or release link, narrow the review to the `@vara-eth/api` release history and the code changes relevant to that package.

### How to analyze an `@vara-eth/api` release

Use this checklist:
1. Identify the current `@vara-eth/api` version in this repo from `package.json` and `pnpm-lock.yaml`.
2. Identify the target `@vara-eth/api` release/tag to review.
3. Compare only the changes relevant to `@vara-eth/api`.
4. Map those changes to the test surface in this repo.

Focus on these areas first:
- client/bootstrap creation in `test/common.ts`
- signer/address access
- `getMirrorClient` / mirror helper API
- injected transaction API
- reply-code types and semantics in `test/helpers/replies.ts`
- transport/reconnect behavior that can affect runtime assertions

Decision rule:
- patch tests if the release changes public API or observable behavior used by the tests
- do not patch tests if the release changes only unrelated internals

Recommended validation order after a TS update:
1. `pnpm exec tsc --noEmit`
2. the smallest relevant Vitest file
3. broader test runs only after the targeted suite is stable

### Rust SDK tests

Files:
- `rust-sdk-tests/**/*.rs`

Source of truth:
- the Rust SDK implementation used by Vara-Eth in the Gear / Vara-Eth upstream repositories
- the exact git revision pinned in `Cargo.lock`

Release pages:
- Gear repository: `https://github.com/gear-tech/gear`

Workflow:
1. Read the currently pinned git revision from `Cargo.lock`.
2. Look specifically at releases, tags, and changes relevant to `ethexe`, not at unrelated repository-wide changes in `gear-tech/gear`.
3. Inspect the upstream SDK implementation at the pinned revision before changing Rust integration tests.
4. If the user asks about newer behavior, or if a newer relevant release exists, compare the pinned revision with the relevant newer upstream release/tag/commit for `ethexe`.
5. Decide whether tests need updates:
   - update tests if SDK method signatures, query semantics, reply handling, router/mirror behavior, or transaction flow changed
   - do not change tests if upstream changes are unrelated to the exercised paths
6. Validate Rust changes with the narrowest useful command first:
   - `cargo test --test <name> --no-run`

Notes:
- For Rust tests, prefer the upstream SDK implementation over assumptions based on older local test code.
- When a failure might come from SDK behavior, inspect the SDK source before changing the test.
- When given a GitHub repository or release link, narrow the review to `ethexe`-relevant changes.

Local execution note:
- When running a single Rust integration test in this repository, start from the `rust-sdk-tests` directory and prefer the short form that the user verified locally:
  - `cargo t -r <test_name> -- --nocapture`
- For example:
  - `cargo t -r manager_distributes_points_across_three_checkers_on_testnet -- --nocapture`
- If that works while a more explicit `cargo test ... --test ...` form behaves differently, prefer the working short form first and only broaden the command if needed for debugging.

### How to analyze an `ethexe` release

Use this checklist:
1. Identify the currently used Gear / `ethexe` revision from `Cargo.lock`.
2. Identify the target release/tag/commit in `https://github.com/gear-tech/gear`.
3. Compare only the changes relevant to `ethexe` and the Rust SDK paths used by this repo.
4. Map those changes to the Rust test surface in this repo.

Focus on these areas first:
- `VaraEthApi` construction and configuration
- router and mirror APIs
- receipt / reply waiting behavior
- query semantics
- nonce / transaction flow changes that affect raw setup
- transport / connection behavior

Decision rule:
- patch tests if the release changes public SDK API or observable behavior used by the tests
- do not patch tests if the release changes only unrelated internals

## Release Comparison Rule

When the user asks whether tests should be changed after a release:
1. identify the dependency and version currently used in this repo
2. inspect the relevant upstream release/tag/changelog
3. compare the changed API/behavior with the current test expectations
4. only then decide whether to patch the tests

The goal is:
- avoid unnecessary test churn
- keep tests aligned with real published behavior
- treat upstream release notes and implementation diffs as the primary evidence

## Release Drift Rule

If there is a relevant upstream release newer than the version or revision currently used by the tests:
1. identify the difference between the current test dependency and the newer release
2. run the narrowest relevant tests against the newer release when feasible
3. report any failures, API mismatches, or behavioral regressions
4. only then decide whether to patch the tests, the dependency version, or both

This rule applies to:
- `@vara-eth/api` for TypeScript tests
- `ethexe` / relevant Gear SDK changes for Rust tests

## Local Debugging Procedure

For running a fully local `ethexe` node, switching `.env` to local endpoints, and fetching the local router address, use the repo skill `local-vara-eth-node`:

- [`local-vara-eth-node`](skills/local-vara-eth-node/SKILL.md)

## Test Launchers

Prefer the repo launchers over ad-hoc manual setup when running the full local or testnet test flow.

### Local

Use:
- `./script/run-tests.sh local rust`
- `./script/run-tests.sh local ts`

Behavior:
- switches `.env` to local RPC endpoints
- builds local wasm artifacts
- uploads local code ids and syncs them into `.env`
- then runs the selected Rust or TypeScript test suite

### Testnet

Use:
- `TESTNET_PRIVATE_KEY=0x... TESTNET_SENDER=0x... ./script/run-tests.sh testnet rust`
- `TESTNET_PRIVATE_KEY=0x... TESTNET_SENDER=0x... ./script/run-tests.sh testnet ts`

Rules:
- testnet runs require an explicit private key
- if `TESTNET_SENDER` is omitted, the launcher may derive it automatically when `cast` is available
- `script/use-testnet-env.sh` should update only RPC/router and optional key/sender values
- `script/use-testnet-env.sh` must not overwrite `TOKEN_ID`, `CHECKER_CODE_ID`, or `MANAGER_CODE_ID`

### Code Id Sync

Use `./script/upload-code-ids.sh` as the canonical way to upload local wasm code and refresh:
- `TOKEN_ID`
- `CHECKER_CODE_ID`
- `MANAGER_CODE_ID`

in the root `.env`.
