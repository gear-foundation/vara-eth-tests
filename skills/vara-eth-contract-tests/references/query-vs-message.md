# Query Vs Message

Use this file when deciding whether a test should call a contract through a query path or through a real message path.

## Query

In this repo, a query usually means a dry-run style call such as:

- Rust SDK: `calculate_reply_for_handle(...)`
- TypeScript API: `varaEthApi.call.program.calculateReplyForHandle(...)`

This is useful for:

- read-style checks
- metadata queries
- checking return values without sending a real message
- cheap validation of a contract API on small or moderate state

## Message

A message means an actual message flow, for example:

- `mirror.sendMessage(...)`
- `send_message_with_receipt(...)`
- injected transactions

Use message-based checks when you need:

- real state transitions
- real reply observation
- receipt and reply validation together
- balance consumption behavior
- realistic integration coverage

## Important Distinction

Do not assume query is equivalent to direct storage reading.

`calculate_reply_for_handle(...)` is execution-based query behavior. It may require:

- runtime preparation
- program data handling
- allocations loading
- wasm instantiation

Because of this, query failures are not always bugs in the getter itself.

## When Query Is A Good Fit

Prefer query when:

- the contract state is still small
- you need to validate a pure read path
- the value being asserted is naturally exposed as a query
- the test is contract-facing rather than runtime-diagnostics-facing

## When Message Is A Better Fit

Prefer message-based progress checks when:

- state is large
- query behavior is suspected to be heavy or unstable
- you need realistic end-to-end behavior
- you need to validate actual asynchronous replies

## Practical Rule For Stress Tests

For stress tests on growing state:

- do not rely exclusively on execution-based queries for progress
- combine receipts, replies, state-hash changes, and executable balance checks
- if query starts failing under large state, investigate whether the failure belongs to the query path rather than to the contract getter itself

## Diagnostic Clues

If the node logs mention traps such as:

- `Not enough gas to handle program data`
- `Not enough gas to obtain program allocations`
- `Not enough gas to instantiate Function section of Wasm module`

then the failure likely belongs to the query/runtime preparation path, not to the business logic of a cheap getter.
