import { getMirrorClient, MirrorClient } from "@vara-eth/api";
import { Hex } from "viem";
import path from "node:path";

import {
  publicClient,
  walletClient,
  varaEthApi,
  ethereumClient,
  wait1Block,
  sails,
} from "../common";
import { readFileSync } from "node:fs";

let vftId: Hex;
let stateHash: Hex;
let codeId: Hex;

const IDL_PATH = "./target/wasm32-gear/release/extended_vft.idl";
const WASM_PATH = "./target/wasm32-gear/release/extended_vft.opt.wasm";

const idlContent = readFileSync(IDL_PATH, "utf-8");
const codeBytes = new Uint8Array(readFileSync(WASM_PATH));

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

    mirror = getMirrorClient(vftId, walletClient, publicClient);

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

    const { waitForReply } = await tx.setupReplyListener();

    const reply = await waitForReply();

    console.log(`[should send init message] Reply received`, reply);

    expect(reply.replyCode).toBe("0x00000000");
    await wait1Block();
  });
});

describe("metadata", () => {
  test("reads name", async () => {
    const queryPayload = sails.services.Vft.queries.Name.encodePayload();
    const queryReply = await varaEthApi.call.program.calculateReplyForHandle(
      ethereumClient.accountAddress,
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
      ethereumClient.accountAddress,
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
      ethereumClient.accountAddress,
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
    mirror = getMirrorClient(vftId, walletClient, publicClient);
    const varaAddress = ethereumClient.accountAddress;
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

    const { waitForReply } = await tx.setupReplyListener();
    const reply = await waitForReply();

    console.log(`[should mint tokens] Reply received:`, reply);

    expect(reply.replyCode).toBe("0x00010000");

    const result = sails.services.Vft.functions.Mint.decodeResult(
      reply.payload,
    );

    console.log("[should mint tokens] Decoded reply:", result);

    expect(result).toBe(true);

    await wait1Block();
  });

  test("should return the increased balance", async () => {
    const varaAddress = ethereumClient.accountAddress;
    const paddedVaraAddress = `0x${varaAddress.slice(2).padStart(64, "0")}`;

    const queryPayload =
      sails.services.Vft.queries.BalanceOf.encodePayload(paddedVaraAddress);

    const queryReply = await varaEthApi.call.program.calculateReplyForHandle(
      ethereumClient.accountAddress,
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
    mirror = getMirrorClient(vftId, walletClient, publicClient);
    const payload = sails.services.Vft.functions.Transfer.encodePayload(
      varaAddress,
      varaAmount,
    );
    const prevStateHash = stateHash ?? (await mirror.stateHash());
    const injected = await varaEthApi.createInjectedTransaction({
      destination: vftId,
      payload, // Encoded message payload
      value: 0n,
    });
    await injected.sendAndWaitForPromise();
    stateHash = await waitForStateHashChange(mirror, prevStateHash);
  });

  test("should return the increased balance", async () => {
    const queryPayload =
      sails.services.Vft.queries.BalanceOf.encodePayload(varaAddress);
  
    const queryReply = await varaEthApi.call.program.calculateReplyForHandle(
      ethereumClient.accountAddress,
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
    mirror = getMirrorClient(vftId, walletClient, publicClient);
    const payload = sails.services.Vft.functions.Mint.encodePayload(
      varaAddress,
      varaAmount,
    );

    const injected = await varaEthApi.createInjectedTransaction({
      destination: vftId,
      payload,
      value: 0n,
    });
    const prevStateHash = stateHash ?? (await mirror.stateHash());
    await injected.sendAndWaitForPromise();
    stateHash = await waitForStateHashChange(mirror, prevStateHash);
  });

  test("should return the increased balance", async () => {
    const queryPayload =
      sails.services.Vft.queries.BalanceOf.encodePayload(varaAddress);

    const queryReply = await varaEthApi.call.program.calculateReplyForHandle(
      ethereumClient.accountAddress,
      vftId,
      queryPayload as `0x${string}`,
    );

    const balance = sails.services.Vft.queries.BalanceOf.decodeResult(
      queryReply.payload,
    );
    expect(balance).toBe("20000000000000");
  });
});
