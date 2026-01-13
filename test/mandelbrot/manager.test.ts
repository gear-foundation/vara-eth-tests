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
import { CONFIG } from "../config";

let managerId: Hex;
const IDL_PATH = "./target/wasm32-gear/release/manager.idl";
const idlContent = readFileSync(IDL_PATH, "utf-8");
describe("create manager", () => {
  const codeId = process.env.MANAGER_CODE_ID! as Hex;
  const TOP_UP_AMOUNT = BigInt(1000 * 1e12);

  let stateHash: Hex;

  test("should check CODE_ID", () => {
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
    managerId = programId;
    console.log("New manager program created:", programId);
  });

  test("should wait for program appeared on Vara.Eth", async () => {
    let appeared = false;

    while (!appeared) {
      await wait1Block();
      const ids = await varaEthApi.query.program.getIds();
      if (ids.includes(managerId)) {
        appeared = true;
      }
    }

    expect(appeared).toBeTruthy();
  });

  test("should approve wvara", async () => {
    const tx = await ethereumClient.wvara.approve(managerId, TOP_UP_AMOUNT);
    const receipt = await tx.sendAndWaitForReceipt();

    expect(receipt.status).toBe("success");
  });

  test("should top up executable balance", async () => {
    let newStateHash: Hex | undefined = undefined;

    const mirror = getMirrorClient(managerId, walletClient, publicClient);

    const unwatch = mirror.watchStateChangedEvent((hash) => {
      newStateHash = hash;
    });

    const tx = await mirror.executableBalanceTopUp(TOP_UP_AMOUNT);

    const receipt = await tx.sendAndWaitForReceipt();

    expect(receipt.status).toBe("success");

    while (!newStateHash) {
      await wait1Block();
    }

    unwatch();

    stateHash = newStateHash;
    console.log(stateHash)
  });

  test("should check executable balance", async () => {
    const state = await varaEthApi.query.program.readState(stateHash);

    expect(BigInt(state.executableBalance)).toBe(TOP_UP_AMOUNT);
  });

  test("should parse idl and create mirror client", () => {
    sails.parseIdl(idlContent);
  });

  test("should send init messages", async () => {
    const mirror = getMirrorClient(managerId, walletClient, publicClient);
    const tx = await mirror.sendMessage(sails.ctors.New.encodePayload());
    const result = await tx.sendAndWaitForReceipt();
    expect(result.status).toBe("success");
    const { waitForReply } = await tx.setupReplyListener();

    const reply = await waitForReply();

    expect(reply.replyCode).toBe("0x00000000");
  });
});

describe("send messages", () => {
  let mirror: MirrorClient;

  const checkerIds: Hex[] = JSON.parse(
    readFileSync(path.join(CONFIG.artifactsDir, "checkers.json"), "utf-8"),
  );

  test("should parse idl and create mirror client", () => {
    sails.parseIdl(idlContent);
    mirror = getMirrorClient(managerId, walletClient, publicClient);
  });

  test("should send GenerateAndStorePoints message", async () => {
    const payload =
      sails.services.Manager.functions.GenerateAndStorePoints.encodePayload(
        100,
        100,
        -2,
        0,
        1,
        0,
        -15,
        2,
        15,
        1,
        30000,
        false,
        false,
        0,
        0,
      );

    const tx = await mirror.sendMessage(payload);
    const receipt = await tx.sendAndWaitForReceipt();

    expect(receipt.status).toBe("success");

    const { waitForReply } = await tx.setupReplyListener();
    const reply = await waitForReply();

    expect(reply.replyCode).toBe("0x00000000");
    expect(reply.payload).toBe("0x");
    expect(reply.value).toBe(0n);
  });
});
