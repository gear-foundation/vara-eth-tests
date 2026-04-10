# Vara-Eth API Patterns

Use this file when the task is about testing the library surface itself, or when a contract test needs lower-level API assertions that are not naturally expressed through Sails.

## When To Prefer This File

Use these patterns when the request is about:

- `createVaraEthApi(...)`
- `WsVaraEthProvider` or `HttpVaraEthProvider`
- `walletClientToSigner(...)`
- `InjectedTx` and `InjectedTxPromise`
- signature generation or recovery
- router view methods
- mirror view methods
- program query readers such as `readState`, `readFullState`, `readQueue`, `readWaitlist`, `readStash`, `readMailbox`, `readPages`
- raw payload testing without IDL parsing

Keep using `vft-patterns.md` when the main job is testing one concrete contract's public behavior through IDL-derived payloads.

## Standard API Setup

```ts
import {
  VaraEthApi,
  WsVaraEthProvider,
  HttpVaraEthProvider,
  createVaraEthApi,
  getMirrorClient,
} from "@vara-eth/api";
import {
  createPublicClient,
  createWalletClient,
  webSocket,
  type Account,
  type Chain,
  type PublicClient,
  type WalletClient,
  type WebSocketTransport,
} from "viem";
import { privateKeyToAccount } from "viem/accounts";

let api: VaraEthApi;
let publicClient: PublicClient<WebSocketTransport, Chain, undefined>;
let walletClient: WalletClient<WebSocketTransport, Chain, Account>;
let signer: ReturnType<typeof walletClientToSigner>;

beforeAll(async () => {
  const transport = webSocket(config.wsRpc);

  publicClient = createPublicClient({ transport }) as PublicClient<
    WebSocketTransport,
    Chain,
    undefined
  >;

  const account = privateKeyToAccount(config.privateKey);

  walletClient = createWalletClient({
    account,
    transport,
  }) as WalletClient<WebSocketTransport, Chain, Account>;

  signer = walletClientToSigner(walletClient);

  api = await createVaraEthApi(
    new WsVaraEthProvider(),
    publicClient,
    config.routerId,
    signer,
  );
});

afterAll(async () => {
  await api.provider.disconnect();
});
```

Use `HttpVaraEthProvider` instead of `WsVaraEthProvider` when the scenario is explicitly about HTTP-backed queries.

## Program Setup Pattern

```ts
let mirror: ReturnType<typeof getMirrorClient>;
let programId: `0x${string}`;

test("should create program", async () => {
  const tx = await api.eth.router.createProgram(config.codeId);
  await tx.sendAndWaitForReceipt();

  programId = await tx.getProgramId();
  expect(programId).toBeDefined();

  mirror = getMirrorClient({ address: programId, signer, publicClient });
});
```

## Wait Until Program Appears

```ts
test(
  "should wait for programId appeared on Vara.Eth",
  async () => {
    let found = false;

    for (let i = 0; i < 30 && !found; i++) {
      const ids = await api.query.program.getIds();
      found = ids.includes(programId);

      if (!found) {
        await waitNBlocks(1);
      }
    }

    expect(found).toBe(true);
  },
  config.longRunningTestTimeout,
);
```

Prefer a bounded loop plus `waitNBlocks(1)` over a tight busy loop.

## Approve WVARA And Assert Allowance

```ts
test("should approve wvara", async () => {
  const tx = await api.eth.wvara.approve(programId, BigInt(10 * 1e12));

  await tx.send();

  const approvalData = await tx.getApprovalLog();
  hasProps(approvalData, ["owner", "spender", "value"]);

  expect(approvalData.value).toEqual(BigInt(10 * 1e12));

  const allowance = await api.eth.wvara.allowance(
    await signer.getAddress(),
    programId,
  );

  expect(allowance).toEqual(BigInt(10 * 1e12));
});
```

## Mirror View Methods

```ts
test("should get router address", async () => {
  const mirrorRouter = await mirror.router();
  expect(mirrorRouter.startsWith("0x")).toBe(true);
});

test("should get state hash", async () => {
  const hash = await mirror.stateHash();
  expect(hash.startsWith("0x")).toBe(true);
});

test("should get nonce", async () => {
  const nonce = await mirror.nonce();
  expect(typeof nonce).toBe("bigint");
});
```

Use this style when covering API surface rather than contract business logic.

## Raw Message Pattern

```ts
test(
  "should send message and receive reply",
  async () => {
    const payload = "0x...";

    const tx = await mirror.sendMessage(payload);
    await tx.send();

    const message = await tx.getMessage();
    hasProps(message, ["id", "source", "payload", "value", "callReply"]);

    const { waitForReply } = await tx.setupReplyListener();
    const reply = await waitForReply();

    expect(reply.replyCode).toBe("0x00010000");
  },
  config.longRunningTestTimeout,
);
```

Use raw payloads only when the request is API-first or when the payload is intentionally fixed by the test.

## Injected Transaction Object Pattern

```ts
test("should set recipient to null by default", async () => {
  const tx = await api.createInjectedTransaction({
    destination: programId,
    payload: "0x",
  });

  expect(tx.recipient).toBeNull();
});

test("should send a message and wait for the promise", async () => {
  const tx = await api.createInjectedTransaction({
    destination: programId,
    payload: "0x...",
  });

  const result = await tx.sendAndWaitForPromise();

  expect(result.txHash).toBeDefined();
  expect(result.code.isSuccess).toBe(true);
  expect(result.signature).toBeDefined();
});
```

## Signature And Promise Validation

```ts
test("should create a correct signature", async () => {
  const account = privateKeyToAccount(PRIVATE_KEY);
  const injected = new InjectedTx(api.provider, api.eth, TX);

  const signature = await account.sign({ hash: injected.hash });
  expect(signature).toBe(expectedSignature);
});

test("should validate promise signature", async () => {
  expect(promise).toBeDefined();
  await expect(promise.validateSignature()).resolves.not.toThrow();
});
```

Prefer deterministic fixtures for pure signing logic. Avoid deploying a program if hash or signature correctness can be tested in isolation.

## Signature Recovery

```ts
test("should correctly recover account from signature", async () => {
  const address = await recoverMessageAddress({
    message: { raw: promise.hash },
    signature: PROMISE.signature,
  });

  expect(address).toBe(account.address);
});
```

## Router-Centric Pattern

```ts
import { getRouterClient, CodeState } from "@vara-eth/api";

let router: RouterClient;

beforeAll(async () => {
  router = getRouterClient({ publicClient, signer, address: config.routerId });
});

test("should request code validation", async () => {
  const tx = await router.requestCodeValidation(code);
  const receipt = await tx.sendAndWaitForReceipt();

  expect(receipt.blockHash).toBeDefined();
});

test("should check that code state is Validated", async () => {
  expect(await router.codeState(codeId)).toBe(CodeState.Validated);
});
```

Use router suites for validation, code state, validator metadata, and program creation behavior.

## Query Readers Pattern

```ts
test("should return full program state with correct field types", async () => {
  const state = await api.query.program.readFullState(stateHash);

  expect(typeof state.program).toBe("object");
  expect(state.canonicalQueue === null || Array.isArray(state.canonicalQueue)).toBe(true);
  expect(state.injectedQueue === null || Array.isArray(state.injectedQueue)).toBe(true);
});
```

For query-reader tests, assert field shape and type stability, not just existence.

## Concurrent Messaging Pattern

```ts
test(
  "should send mixed injected and mirror messages concurrently",
  async () => {
    const [injected1, injected2] = await Promise.all([
      api.createInjectedTransaction({ destination: programId, payload: PAYLOAD }),
      api.createInjectedTransaction({ destination: programId, payload: PAYLOAD }),
    ]);

    const startingNonce = await publicClient.getTransactionCount({
      address: await signer.getAddress(),
      blockTag: "pending",
    });

    const [mirror1, mirror2] = await Promise.all([
      mirror.sendMessage(PAYLOAD, 0n, { nonce: startingNonce }),
      mirror.sendMessage(PAYLOAD, 0n, { nonce: startingNonce + 1 }),
    ]);

    await Promise.all([
      injected1.sendAndWaitForPromise(),
      injected2.sendAndWaitForPromise(),
      mirror1.send(),
      mirror2.send(),
    ]);
  },
  config.longRunningTestTimeout,
);
```

Only use this style when the task explicitly asks for concurrency, nonce handling, or event aggregation.
