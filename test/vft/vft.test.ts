import { getMirrorClient, MirrorClient } from "@vara-eth/api";
import { Hex } from "viem";

import {
  publicClient,
  walletClient,
  varaEthApi,
  ethereumClient,
  accountAddress,
  wait1Block,
  sails,
  reconnect,
  isRetryableConnectionError,
} from "../common";
import { readFileSync } from "node:fs";
import {
  expectAutoSuccessPromise,
  expectAutoSuccessReply,
  expectErrorReplyCode,
  expectManualSuccessPromise,
  expectManualSuccessReply,
} from "../helpers/replies";

let vftId: Hex;
let stateHash: Hex;
let codeId: Hex;

const IDL_PATH = "./artifacts/idl/extended_vft.idl";

const idlContent = readFileSync(IDL_PATH, "utf-8");

async function waitForReplyWithReconnect(
  address: Hex,
  setupReplyListener: () => Promise<{
    blockNumber: number;
    message: { id: Hex };
    waitForReply: () => Promise<any>;
  }>,
  label: string,
) {
  const listener = await setupReplyListener();

  try {
    return await listener.waitForReply();
  } catch (error) {
    if (!isRetryableConnectionError(error)) {
      throw error;
    }

    console.warn(
      `[${label}] Reply listener interrupted, reconnecting and waiting for reply again`,
      error,
    );

    await reconnect();

    const freshMirror = getMirrorClient({
      address,
      publicClient,
      signer: ethereumClient.signer,
    });

    return freshMirror.waitForReply(
      listener.message.id,
      BigInt(listener.blockNumber),
    );
  }
}

async function createDefaultInjectedTransaction(
  payload: Hex,
  label: string,
) {
  const injected = await varaEthApi.createInjectedTransaction({
    destination: vftId,
    payload,
    value: 0n,
  });

  const recipient = injected.setDefaultValidator();
  console.log(
    `[${label}] Prepared injected transaction`,
    {
      recipient,
      messageId: injected.messageId,
      txHash: injected.txHash,
      referenceBlock: injected.referenceBlock,
    },
  );

  return injected;
}

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

function decodeScaleString(payload: Hex): string {
  const bytes = Buffer.from(payload.slice(2), "hex");
  const len = bytes[0] >> 2;
  return bytes.subarray(1, 1 + len).toString("utf-8");
}

describe("create token", () => {
  const envCodeId = process.env.TOKEN_ID as Hex;
  const TOP_UP_AMOUNT = BigInt(100 * 1e12);
  let mirror: MirrorClient;

  test("should check CODE_ID", () => {
    codeId = envCodeId;
    expect(codeId).toBeDefined();
    expect(codeId).toHaveLength(66);
    console.log(`Using code id ${codeId} for manager program`);
  });

  test("should create program", async () => {
    const tx = await ethereumClient.router.createProgram(codeId);

    const receipt = await tx.sendAndWaitForReceipt();

    expect(receipt.status).toBe("success");

    const programId = await tx.getProgramId();

    expect(programId).toBeDefined();
    vftId = programId;
    console.log("New vft program created:", programId);
  });

  test("should wait for program appeared on Vara.Eth", async () => {
    let appeared = false;

    while (!appeared) {
      await wait1Block();
      const ids = await varaEthApi.query.program.getIds();
      if (ids.includes(vftId)) {
        appeared = true;
      }
    }

    expect(appeared).toBeTruthy();
  });

  test("should approve wvara", async () => {
    const tx = await ethereumClient.wvara.approve(vftId, TOP_UP_AMOUNT);
    const receipt = await tx.sendAndWaitForReceipt();

    expect(receipt.status).toBe("success");
  });

  test("should top up executable balance", async () => {
    let newStateHash: Hex | undefined = undefined;

    mirror = getMirrorClient({
      address: vftId,
      publicClient,
      signer: ethereumClient.signer,
    });

    const unwatch = mirror.watchStateChangedEvent((hash) => {
      console.log(
        `[should top up executable balance] State changed: ${newStateHash}`,
      );
      newStateHash = hash;
    });

    const tx = await mirror.executableBalanceTopUp(TOP_UP_AMOUNT);

    const receipt = await tx.sendAndWaitForReceipt();

    console.log(
      `[should top up executable balance] Got receipt: ${receipt.status}`,
    );

    expect(receipt.status).toBe("success");

    console.log("stateHash", stateHash)
    while (!newStateHash) {
      await wait1Block();
    }

    unwatch();

    stateHash = newStateHash;
    console.log("stateHash", stateHash)
  });

  test("should check that executable balance is equal to TOP_UP_AMOUNT", async () => {
    const state = await varaEthApi.query.program.readState(stateHash);

    expect(BigInt(state.executableBalance)).toBe(TOP_UP_AMOUNT);
  });

  test("should parse idl", () => {
    sails.parseIdl(idlContent);
  });

  test("should send init message", async () => {
    const payload = sails.ctors.Init.encodePayload("Name", "Symbol", "12");

    console.log(`[should send init message] Sending message: ${payload}`);

    const tx = await mirror.sendMessage(payload);

    const txHash = await tx.send();

    console.log(`[should send init message] Tx hash: ${txHash}`);
    console.log(`[should send init message] Message`, await tx.getMessage());

    const reply = await waitForReplyWithReconnect(
      vftId,
      () => tx.setupReplyListener(),
      "should send init message",
    );

    console.log(`[should send init message] Reply received`, reply);

    expectAutoSuccessReply(reply);
    await wait1Block();
  });
});

describe("metadata", () => {
  test("reads name", async () => {
    const queryPayload = sails.services.Vft.queries.Name.encodePayload();
    const queryReply = await varaEthApi.call.program.calculateReplyForHandle(
      accountAddress,
      vftId,
      queryPayload as `0x${string}`,
    );

    const name = sails.services.Vft.queries.Name.decodeResult(
      queryReply.payload,
    );
    expect(name).toBe("Name");
  });

  test("reads symbol", async () => {
    const queryPayload = sails.services.Vft.queries.Symbol.encodePayload();
    const queryReply = await varaEthApi.call.program.calculateReplyForHandle(
      accountAddress,
      vftId,
      queryPayload as `0x${string}`,
    );

    const symbol = sails.services.Vft.queries.Symbol.decodeResult(
      queryReply.payload,
    );
    expect(symbol).toBe("Symbol");
  });

  test("reads decimals", async () => {
    const queryPayload = sails.services.Vft.queries.Decimals.encodePayload();
    const queryReply = await varaEthApi.call.program.calculateReplyForHandle(
      accountAddress,
      vftId,
      queryPayload as `0x${string}`,
    );

    const decimals = sails.services.Vft.queries.Decimals.decodeResult(
      queryReply.payload,
    );
    expect(decimals).toBe(12);
  });
});

describe("send messages: mint", () => {
  let mirror: MirrorClient;
  const varaAmount = "10000000000000";

  test("should mint tokens", async () => {
    mirror = getMirrorClient({
      address: vftId,
      publicClient,
      signer: ethereumClient.signer,
    });
    const varaAddress = accountAddress;
    const paddedVaraAddress = `0x${varaAddress.slice(2).padStart(64, "0")}`;

    const payload = sails.services.Vft.functions.Mint.encodePayload(
      paddedVaraAddress,
      varaAmount,
    );

    console.log(`[should mint tokens] Sending message ${payload}`);

    const tx = await mirror.sendMessage(payload);

    const receipt = await tx.sendAndWaitForReceipt();

    console.log(`[should mint tokens] Receipt received: ${receipt.status}`);

    expect(receipt.status).toBe("success");

    console.log(`[should mint tokens] Sent message:`, await tx.getMessage());

    const reply = await waitForReplyWithReconnect(
      vftId,
      () => tx.setupReplyListener(),
      "should mint tokens",
    );

    console.log(`[should mint tokens] Reply received:`, reply);

    expectManualSuccessReply(reply);

    const result = sails.services.Vft.functions.Mint.decodeResult(
      reply.payload,
    );

    console.log("[should mint tokens] Decoded reply:", result);

    expect(result).toBe(true);

    await wait1Block();
  });

  test("should return the increased balance", async () => {
    const varaAddress = accountAddress;
    const paddedVaraAddress = `0x${varaAddress.slice(2).padStart(64, "0")}`;

    const queryPayload =
      sails.services.Vft.queries.BalanceOf.encodePayload(paddedVaraAddress);

    const queryReply = await varaEthApi.call.program.calculateReplyForHandle(
      accountAddress,
      vftId,
      queryPayload as `0x${string}`,
    );

    const balance = sails.services.Vft.queries.BalanceOf.decodeResult(
      queryReply.payload,
    );
    expect(balance).toBe(varaAmount);
  });

  test("program executable balance decreases after calls", async () => {
    let state = await varaEthApi.query.program.readState(stateHash);

    const balanceBefore = state.executableBalance;
    const newStateHash = await mirror.stateHash();
    state = await varaEthApi.query.program.readState(newStateHash);
    const balanceAfter = state.executableBalance;
    expect(balanceAfter).toBeLessThan(balanceBefore);
  });
});

describe("injected txs: transfer", () => {
  let mirror: MirrorClient;
  const varaAddress =
    "0xae665500b487d538c34dee6c68ba737f1add21ed275fcca75523342639abc536";
  const varaAmount = "10000000000000";

  test("should transfer tokens", async () => {
    mirror = getMirrorClient({
      address: vftId,
      publicClient,
      signer: ethereumClient.signer,
    });
    const payload = sails.services.Vft.functions.Transfer.encodePayload(
      varaAddress,
      varaAmount,
    );
    const prevStateHash = stateHash ?? (await mirror.stateHash());
    const injected = await createDefaultInjectedTransaction(
      payload,
      "should transfer tokens",
    );
    const reply = await injected.sendAndWaitForPromise();
    expectManualSuccessPromise(reply);
    const result = sails.services.Vft.functions.Transfer.decodeResult(reply.payload);
    expect(result).toBe(true);
    stateHash = await waitForStateHashChange(mirror, prevStateHash);
  });

  test("should return the increased balance", async () => {
    const queryPayload =
      sails.services.Vft.queries.BalanceOf.encodePayload(varaAddress);
  
    const queryReply = await varaEthApi.call.program.calculateReplyForHandle(
      accountAddress,
      vftId,
      queryPayload as `0x${string}`,
    );
    const balance = sails.services.Vft.queries.BalanceOf.decodeResult(
      queryReply.payload,
    );
    expect(balance).toBe(varaAmount);
  });
});

describe("injected txs: mint", () => {
  let mirror: MirrorClient;
  const varaAddress =
    "0xae665500b487d538c34dee6c68ba737f1add21ed275fcca75523342639abc536";
  const varaAmount = "10000000000000";
  test("should mint tokens", async () => {
    mirror = getMirrorClient({
      address: vftId,
      publicClient,
      signer: ethereumClient.signer,
    });
    const payload = sails.services.Vft.functions.Mint.encodePayload(
      varaAddress,
      varaAmount,
    );
    const prevStateHash = stateHash ?? (await mirror.stateHash());
    const injected = await createDefaultInjectedTransaction(
      payload,
      "should mint tokens",
    );
    const reply = await injected.sendAndWaitForPromise();
    expectManualSuccessPromise(reply);
    const result = sails.services.Vft.functions.Mint.decodeResult(reply.payload);
    expect(result).toBe(true);
    stateHash = await waitForStateHashChange(mirror, prevStateHash);
  });

  test("should return the increased balance", async () => {
    const queryPayload =
      sails.services.Vft.queries.BalanceOf.encodePayload(varaAddress);

    const queryReply = await varaEthApi.call.program.calculateReplyForHandle(
      accountAddress,
      vftId,
      queryPayload as `0x${string}`,
    );

    const balance = sails.services.Vft.queries.BalanceOf.decodeResult(
      queryReply.payload,
    );
    expect(balance).toBe("20000000000000");
  });
});

describe("negative replies", () => {
  test("should return error reply on transfer with insufficient balance", async () => {
    const mirror = getMirrorClient({
      address: vftId,
      publicClient,
      signer: ethereumClient.signer,
    });
    const otherAddress = `0x${"dead".padStart(40, "0")}`;
    const payload = sails.services.Vft.functions.Transfer.encodePayload(
      `0x${otherAddress.slice(2).padStart(64, "0")}`,
      "20000000000001",
    );

    const balanceQuery =
      sails.services.Vft.queries.BalanceOf.encodePayload(
        `0x${accountAddress.slice(2).padStart(64, "0")}`,
      );
    const totalSupplyQuery = sails.services.Vft.queries.TotalSupply.encodePayload();

    const balanceBeforeReply =
      await varaEthApi.call.program.calculateReplyForHandle(
        accountAddress,
        vftId,
        balanceQuery as `0x${string}`,
      );
    const totalSupplyBeforeReply =
      await varaEthApi.call.program.calculateReplyForHandle(
        accountAddress,
        vftId,
        totalSupplyQuery as `0x${string}`,
      );

    const balanceBefore = sails.services.Vft.queries.BalanceOf.decodeResult(
      balanceBeforeReply.payload,
    );
    const totalSupplyBefore = sails.services.Vft.queries.TotalSupply.decodeResult(
      totalSupplyBeforeReply.payload,
    );

    const tx = await mirror.sendMessage(payload);
    const receipt = await tx.sendAndWaitForReceipt();
    expect(receipt.status).toBe("success");

    const reply = await waitForReplyWithReconnect(
      vftId,
      () => tx.setupReplyListener(),
      "negative transfer insufficient balance",
    );

    expectErrorReplyCode(reply.replyCode);
    expect(decodeScaleString(reply.payload as Hex)).toBe("InsufficientBalance");
    expect(reply.value).toBe(0n);

    await wait1Block();

    const balanceAfterReply =
      await varaEthApi.call.program.calculateReplyForHandle(
        accountAddress,
        vftId,
        balanceQuery as `0x${string}`,
      );
    const totalSupplyAfterReply =
      await varaEthApi.call.program.calculateReplyForHandle(
        accountAddress,
        vftId,
        totalSupplyQuery as `0x${string}`,
      );

    const balanceAfter = sails.services.Vft.queries.BalanceOf.decodeResult(
      balanceAfterReply.payload,
    );
    const totalSupplyAfter = sails.services.Vft.queries.TotalSupply.decodeResult(
      totalSupplyAfterReply.payload,
    );

    expect(balanceAfter).toBe(balanceBefore);
    expect(totalSupplyAfter).toBe(totalSupplyBefore);
  });
});
