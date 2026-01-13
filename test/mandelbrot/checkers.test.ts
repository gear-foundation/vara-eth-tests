import { mkdirSync, writeFileSync } from "node:fs";
import { getMirrorClient } from "@vara-eth/api";
import { Hex } from "viem";
import path from "node:path";
import { readFileSync } from "node:fs";
import { Sails } from "sails-js";
import {
  publicClient,
  walletClient,
  varaEthApi,
  ethereumClient,
  wait1Block,
  sails,
} from "../common";
import { CONFIG } from "../config";

const IDL_PATH = "./target/wasm32-gear/release/mandelbrot_checker.idl";
const idlContent = readFileSync(IDL_PATH, "utf-8");

describe("create checkers", () => {
  const codeId = process.env.CHECKER_CODE_ID! as Hex;
  const PROGRAM_COUNT = 5;
  const checkerIds: Hex[] = [];
  const TOP_UP_AMOUNT = BigInt(100 * 1e12);
  const stateHashes = new Map<Hex, Hex>();

  test("should check CODE_ID", () => {
    expect(codeId).toBeDefined();
    expect(codeId).toHaveLength(66);
    console.log(`Using code id ${codeId} for checker programs`);
  });

  test("should create programs", async () => {
    const promises = [];

    for (let i = 0; i < PROGRAM_COUNT; i++) {
      const tx = await ethereumClient.router.createProgram(codeId);

      promises.push(
        tx.sendAndWaitForReceipt().then((receipt) => {
          return { status: receipt.status, programId: tx.getProgramId() };
        }),
      );
    }

    const result = await Promise.all(promises);

    expect(result).toHaveLength(PROGRAM_COUNT);

    for (const item of result) {
      expect(item.status).toBe("success");
      const programId = await item.programId;
      expect(programId).toBeDefined();
      checkerIds.push(programId);
      console.log("New checker program created:", programId);
    }
  });

  test("should check number of created checkers", () => {
    expect(checkerIds).toHaveLength(PROGRAM_COUNT);
  });

  test("should wait for programs appeared on Vara.Eth", async () => {
    let allProgramsOnVaraEth = false;
    while (!allProgramsOnVaraEth) {
      await wait1Block();
      const ids = await varaEthApi.query.program.getIds();
      allProgramsOnVaraEth = checkerIds.every((id) => ids.includes(id));
    }

    expect(allProgramsOnVaraEth).toBeTruthy();
  });

  test("should approve wvara", async () => {
    const promises = [];

    for (const id of checkerIds) {
      const tx = await ethereumClient.wvara.approve(id, TOP_UP_AMOUNT);
      promises.push(tx.sendAndWaitForReceipt());
    }

    const result = await Promise.all(promises);

    for (const item of result) {
      expect(item.status).toBe("success");
    }
  });

  test("should top up executable balance", async () => {
    const promises = [];
    const subscriptions = [];

    let numberOfStateHashes = 0;

    for (const id of checkerIds) {
      const mirror = getMirrorClient(id, walletClient, publicClient);
      const unwatch = mirror.watchStateChangedEvent((newStateHash) => {
        stateHashes.set(id, newStateHash);
        numberOfStateHashes++;
      });
      subscriptions.push(unwatch);
      const tx = await mirror.executableBalanceTopUp(TOP_UP_AMOUNT);
      promises.push(tx.sendAndWaitForReceipt());
    }

    const result = await Promise.all(promises);

    for (const item of result) {
      expect(item.status).toBe("success");
    }

    while (numberOfStateHashes < PROGRAM_COUNT) {
      await wait1Block();
    }

    subscriptions.map((unwatch) => {
      unwatch();
    });
  });

  test("should check executable balance", async () => {
    for (const id of checkerIds) {
      expect(stateHashes.has(id)).toBeTruthy();
      const state = await varaEthApi.query.program.readState(
        stateHashes.get(id)!,
      );

      expect(BigInt(state.executableBalance)).toBe(TOP_UP_AMOUNT);
    }
  });


  test("should send init messages", async () => {
    const promises = [];
    let sails = pa
    for (const id of checkerIds) {
      const mirror = getMirrorClient(id, walletClient, publicClient);
      const tx = await mirror.sendMessage(sails.ctors.Init.encodePayload())
      const result = await tx.sendAndWaitForReceipt();
      expect(result.status).toBe("success");
      const { waitForReply } = await tx.setupReplyListener();
      promises.push(waitForReply());
    }

    const replies = await Promise.all(promises);

    for (const reply of replies) {
      expect(reply.replyCode).toBe("0x00000000");
    }
  });

  test("should save checker addresses", () => {
    mkdirSync("./tmp", { recursive: true });
    writeFileSync(
      path.join(CONFIG.artifactsDir, "checkers.json"),
      JSON.stringify(checkerIds),
    );
  });
});
