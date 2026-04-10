# Reply Testing

Use this file when writing assertions around `replyCode`, injected promise `code`, or negative cases involving failed execution and unavailable actors.

## Core Rule

Do not treat Ethereum receipt success and Vara reply success as the same thing.

- `receipt.status === "success"` means the Ethereum-side transaction was accepted and executed.
- `replyCode` or injected promise `code` tells you what happened to the handled message on the Vara side.

A test can have:

- successful receipt and successful reply
- successful receipt and error reply
- rejected promise or thrown RPC error before a reply is available

## Reply Code Shape

Reply codes are 4 bytes:

- byte `0`: top-level kind
- byte `1`: success or error subtype
- bytes `2..3`: reason details

Practical values:

- `0x00000000` = success, auto reply
- `0x00010000` = success, manual reply

For success replies:

- auto success means the runtime returned a success reply automatically
- manual success means the actor explicitly created a reply

## Auto Vs Manual Success

Use `0x00000000` when the contract does not explicitly reply to the handled message.

Typical cases:

- init completed successfully and returned no actor-created reply payload
- a handle function only mutates state
- a handle function emits or sends another message but does not explicitly return a reply

Use `0x00010000` when the actor explicitly returns a success reply.

Typical cases:

- a service function returns a value
- the contract explicitly responds through the reply path
- the decoded reply payload is meaningful and should be asserted

## Contract Reading Rule

Before asserting `replyCode`, inspect the contract behavior:

1. Does the handler explicitly produce a reply?
2. Does it only send a separate message to the caller or another actor?
3. Does the IDL function return a value or `null`?

Important:

- `-> null` in the IDL does not automatically guarantee manual success
- if the contract sends a message with `msg::send...` but does not explicitly reply, the observed result is often auto success

## Assertion Patterns

### Auto Success

```ts
const reply = await waitForReply();

expect(reply.replyCode).toBe("0x00000000");
expect(reply.payload).toBe("0x");
expect(reply.value).toBe(0n);
```

Use the same expectation for injected promise results when the promise code is auto success:

```ts
const result = await injected.sendAndWaitForPromise();

expect(result.code).toBe("0x00000000");
expect(result.payload).toBe("0x");
expect(result.value).toBe(0n);
```

### Manual Success

```ts
const reply = await waitForReply();

expect(reply.replyCode).toBe("0x00010000");

const decoded = sails.services.MyService.functions.MyCall.decodeResult(
  reply.payload,
);

expect(decoded).toBe(true);
```

Use this only when the contract actually returns a meaningful actor-created reply.

## What To Assert In Positive Cases

Choose the narrowest useful assertion:

1. Ethereum receipt succeeded
2. reply code matches the contract behavior
3. reply payload shape matches expectations
4. returned value is correct
5. side effects occurred:
state hash changed, balance changed, query result changed, reply listener fired, or emitted message became observable

Do not stop at `receipt.status === "success"` if the point of the test is Vara-side behavior.

## Negative Case Categories

Split negative scenarios into three buckets.

### 1. RPC Or Client-Level Rejection

Use `rejects.toThrow(...)` when the request is invalid before a meaningful reply exists.

Examples:

- invalid signature
- malformed params
- unsupported injected tx value
- destination mismatch during signing

Pattern:

```ts
await expect(tx.sendAndWaitForPromise()).rejects.toThrow();
```

### 2. Successful Receipt But Error Reply

Use this when the transaction was accepted, but the handled message failed inside Vara execution.

Examples:

- userspace panic
- uninitialized actor
- program not created
- backend execution failure

Pattern:

```ts
const { waitForReply } = await tx.setupReplyListener();
const reply = await waitForReply();

expect(reply.replyCode).not.toBe("0x00000000");
expect(reply.replyCode).not.toBe("0x00010000");
```

If the project exposes a reply-code parser, prefer asserting parsed semantics, not just raw bytes.

### 3. Successful Reply With Business-Level Failure Encoded In Payload

Some contracts model failure as a valid manual success reply with an error-like payload.

Examples:

- `Result::Err(...)`
- boolean `false`
- domain-specific error enum in the payload

Pattern:

```ts
expect(reply.replyCode).toBe("0x00010000");

const decoded = sails.services.MyService.functions.MyCall.decodeResult(
  reply.payload,
);

expect(decoded).toEqual({ Err: ... });
```

Do not mistake these for transport or runtime failures.

## Negative Tests Worth Adding

For new contracts or API surfaces, consider at least one negative case from the relevant category:

- call before init if the contract requires initialization
- invalid destination or invalid program id
- malformed injected transaction signature
- unsupported non-zero value for injected tx if the API rejects it
- payload that should trigger a contract panic or reject path
- query against a program that does not exist

## Default Heuristic

Use this decision tree:

1. If the operation itself should be rejected by the client or RPC layer, assert `rejects.toThrow(...)`.
2. If the transaction should be accepted but execution should fail, assert on error reply semantics.
3. If the contract models failure in its returned payload, assert manual success plus decoded failure payload.

## Current Repo Example

For `mandelbrot-checker`, `CheckMandelbrotPoints` sends a separate message but does not explicitly create a reply for the handled message, so the correct expectation is:

```ts
expect(reply.replyCode).toBe("0x00000000");
```

This is auto success, not manual success.
