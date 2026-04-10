# VFT Patterns

Use this file as the canonical pattern source for new contract tests in this repo.

## Core Imports

```ts
import { getMirrorClient, MirrorClient } from "@vara-eth/api";
import { Hex } from "viem";
import { readFileSync } from "node:fs";

import {
  publicClient,
  walletClient,
  varaEthApi,
  ethereumClient,
  wait1Block,
  sails,
} from "../common";
```

Adjust the relative path from `../common` if the file sits deeper under `test/`.

## Standard Module State

```ts
let programId: Hex;
let stateHash: Hex;

const IDL_PATH = "./target/wasm32-gear/release/<contract>.idl";
const idlContent = readFileSync(IDL_PATH, "utf-8");
```

Add `codeId` as a module variable only if later describe blocks need it.

## Bounded State-Hash Polling

```ts
async function waitForStateHashChange(
  mirror: MirrorClient,
  prevStateHash: Hex,
  maxBlocks = 30,
): Promise<Hex> {
  for (let i = 0; i < maxBlocks; i++) {
    await wait1Block();
    const nextStateHash = await mirror.stateHash();
    if (nextStateHash !== prevStateHash) {
      return nextStateHash;
    }
  }

  throw new Error(
    `State hash did not change within ${maxBlocks} blocks. Previous state hash: ${prevStateHash}`,
  );
}
```

Prefer this over open-ended polling when the contract should emit a visible state transition.

## Deployment Block

```ts
describe("create <contract>", () => {
  const envCodeId = process.env.<ENV_NAME> as Hex;
  const TOP_UP_AMOUNT = BigInt(100 * 1e12);
  let mirror: MirrorClient;

  test("should check CODE_ID", () => {
    expect(envCodeId).toBeDefined();
    expect(envCodeId).toHaveLength(66);
  });

  test("should create program", async () => {
    const tx = await ethereumClient.router.createProgram(envCodeId);
    const receipt = await tx.sendAndWaitForReceipt();

    expect(receipt.status).toBe("success");

    programId = await tx.getProgramId();
    expect(programId).toBeDefined();
  });

  test("should wait for program appeared on Vara.Eth", async () => {
    let appeared = false;

    for (let i = 0; i < 30 && !appeared; i++) {
      await wait1Block();
      const ids = await varaEthApi.query.program.getIds();
      appeared = ids.includes(programId);
    }

    expect(appeared).toBe(true);
  });

  test("should approve wvara", async () => {
    const tx = await ethereumClient.wvara.approve(programId, TOP_UP_AMOUNT);
    const receipt = await tx.sendAndWaitForReceipt();

    expect(receipt.status).toBe("success");
  });

  test("should top up executable balance", async () => {
    mirror = getMirrorClient(programId, walletClient, publicClient);

    let nextHash: Hex | undefined;
    const unwatch = mirror.watchStateChangedEvent((hash) => {
      nextHash = hash;
    });

    const tx = await mirror.executableBalanceTopUp(TOP_UP_AMOUNT);
    const receipt = await tx.sendAndWaitForReceipt();

    expect(receipt.status).toBe("success");

    for (let i = 0; i < 30 && !nextHash; i++) {
      await wait1Block();
    }

    unwatch();

    expect(nextHash).toBeDefined();
    stateHash = nextHash!;
  });

  test("should check executable balance", async () => {
    const state = await varaEthApi.query.program.readState(stateHash);
    expect(BigInt(state.executableBalance)).toBe(TOP_UP_AMOUNT);
  });
});
```

## IDL Parse And Init Message

```ts
test("should parse idl", () => {
  sails.parseIdl(idlContent);
});

test("should send init message", async () => {
  const payload = sails.ctors.Init.encodePayload(/* ctor args */);
  const tx = await mirror.sendMessage(payload);
  const receipt = await tx.sendAndWaitForReceipt();

  expect(receipt.status).toBe("success");

  const { waitForReply } = await tx.setupReplyListener();
  const reply = await waitForReply();

  expect(reply.replyCode).toBe("0x00000000");
});
```

Replace `Init` with the real constructor or service function exposed by the contract IDL.

## Query Pattern

```ts
const queryPayload = sails.services.<Service>.queries.<Query>.encodePayload(
  /* optional args */
);

const queryReply = await varaEthApi.call.program.calculateReplyForHandle(
  ethereumClient.accountAddress,
  programId,
  queryPayload as `0x${string}`,
);

const result = sails.services.<Service>.queries.<Query>.decodeResult(
  queryReply.payload,
);

expect(result).toBe(/* expected value */);
```

## Direct Message Pattern

```ts
const payload = sails.services.<Service>.functions.<Function>.encodePayload(
  /* args */
);

const tx = await mirror.sendMessage(payload);
const receipt = await tx.sendAndWaitForReceipt();

expect(receipt.status).toBe("success");

const { waitForReply } = await tx.setupReplyListener();
const reply = await waitForReply();

expect(reply.replyCode).toBe("0x00010000");

const result = sails.services.<Service>.functions.<Function>.decodeResult(
  reply.payload,
);

expect(result).toBe(true);
```

Reply codes depend on the contract, so confirm them from observed behavior or contract conventions before hardcoding.

## Injected Transaction Pattern

```ts
const payload = sails.services.<Service>.functions.<Function>.encodePayload(
  /* args */
);

const prevStateHash = stateHash ?? (await mirror.stateHash());

const injected = await varaEthApi.createInjectedTransaction({
  destination: programId,
  payload,
  value: 0n,
});

await injected.sendAndWaitForPromise();
stateHash = await waitForStateHashChange(mirror, prevStateHash);
```

Use this when the point of the test is to verify injected transaction flow rather than the mirror client path.
