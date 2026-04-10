# Environment Matrix

Use this file to decide whether a test should run against a local Vara-Eth node or against testnet.

## Local

Prefer local runs when the goal is:

- contract debugging
- payload debugging
- balance and top-up debugging
- reply-code investigation
- quick iteration on test logic
- validating freshly built local wasm artifacts

Local workflow in this repo:

- start the node using `skills/local-vara-eth-node/SKILL.md`
- switch `.env` to local values
- build wasm artifacts
- upload fresh local code ids
- run Rust or TypeScript tests

Use the launcher:

- `./script/run-tests.sh local rust`
- `./script/run-tests.sh local ts`

## Testnet

Prefer testnet runs when the goal is:

- checking real remote behavior
- validating that flows work outside the local dev stack
- comparing local behavior with validator-backed infrastructure
- reproducing issues that do not appear on local dev mode
- checking compatibility of tests against currently deployed environments

Use the launcher:

- `TESTNET_PRIVATE_KEY=0x... TESTNET_SENDER=0x... ./script/run-tests.sh testnet rust`
- `TESTNET_PRIVATE_KEY=0x... TESTNET_SENDER=0x... ./script/run-tests.sh testnet ts`

## Decision Rule

Start with local when:

- you changed contract code
- you changed test code
- you need logs
- you need to rebuild and re-upload code ids frequently

Escalate to testnet when:

- the local scenario is already stable
- the failure may depend on remote infrastructure
- the user explicitly asks for testnet validation

## Interpretation Rule

Do not assume local and testnet failures mean the same thing.

Common differences:

- timing and polling behavior
- executable balance consumption patterns
- query behavior under larger state
- code validation and upload responses
- transport and reconnect behavior

Use local for fast diagnosis and testnet for confirmation.
