import { getMirrorClient, MirrorClient } from "@vara-eth/api";
import { readFileSync } from "node:fs";
import { Hex } from "viem";

import {
  publicClient,
  walletClient,
  varaEthApi,
  ethereumClient,
  accountAddress,
  wait1Block,
  sails,
} from "../common";
import {
  expectAutoSuccessPromise,
  expectAutoSuccessReply,
} from "../helpers/replies";

const IDL_PATH = "./artifacts/idl/mandelbrot_checker.idl";
const idlContent = readFileSync(IDL_PATH, "utf-8");

const TOP_UP_AMOUNT = BigInt(100 * 1e12);
const MAX_WAIT_BLOCKS = 30;
const EMPTY_POINTS_U16 = [0];

let checkerId: Hex;
let stateHash: Hex;
let mirror: MirrorClient;

async function waitForProgramOnVaraEth(programId: Hex) {
  for (let i = 0; i < MAX_WAIT_BLOCKS; i++) {
    await wait1Block();
    const ids = await varaEthApi.query.program.getIds();

    if (ids.includes(programId)) {
      return;
    }
  }

  throw new Error(
    `Program did not appear on Vara.Eth within ${MAX_WAIT_BLOCKS} blocks`,
  );
}

async function waitForTopUpStateHash() {
  for (let i = 0; i < MAX_WAIT_BLOCKS; i++) {
    if (stateHash) {
      return;
    }

    await wait1Block();
  }

  throw new Error(
    `Executable-balance top-up state hash did not arrive within ${MAX_WAIT_BLOCKS} blocks`,
  );
}

describe("create checker", () => {
  const codeId = process.env.CHECKER_CODE_ID! as Hex;

  test("should check CODE_ID", () => {
    expect(codeId).toBeDefined();
    expect(codeId).toHaveLength(66);
  });

  test("should create program", async () => {
    const tx = await ethereumClient.router.createProgram(codeId);
    const receipt = await tx.sendAndWaitForReceipt();

    expect(receipt.status).toBe("success");

    const programId = await tx.getProgramId();
    expect(programId).toBeDefined();

    checkerId = programId;
    mirror = getMirrorClient({
      address: programId,
      publicClient,
      signer: ethereumClient.signer,
    });
  });

  test("should wait for program appeared on Vara.Eth", async () => {
    await waitForProgramOnVaraEth(checkerId);
  });

  test("should approve wvara", async () => {
    const tx = await ethereumClient.wvara.approve(checkerId, TOP_UP_AMOUNT);
    const receipt = await tx.sendAndWaitForReceipt();

    expect(receipt.status).toBe("success");
  });

  test("should top up executable balance", async () => {
    const unwatch = mirror.watchStateChangedEvent((nextStateHash) => {
      stateHash = nextStateHash;
    });

    try {
      const tx = await mirror.executableBalanceTopUp(TOP_UP_AMOUNT);
      const receipt = await tx.sendAndWaitForReceipt();

      expect(receipt.status).toBe("success");
      await waitForTopUpStateHash();
    } finally {
      unwatch();
    }
  });

  test("should check executable balance", async () => {
    const state = await varaEthApi.query.program.readState(stateHash);
    expect(BigInt(state.executableBalance)).toBe(TOP_UP_AMOUNT);
  });

  test("should parse idl", () => {
    sails.parseIdl(idlContent);
  });

  test("should send init message", async () => {
    const tx = await mirror.sendMessage(sails.ctors.Init.encodePayload());
    const receipt = await tx.sendAndWaitForReceipt();

    expect(receipt.status).toBe("success");

    const { waitForReply } = await tx.setupReplyListener();
    const reply = await waitForReply();

    expectAutoSuccessReply(reply);
  });

  test("should check mandelbrot points via mirror message", async () => {
    const payload =
      sails.services.MandelbrotChecker.functions.CheckMandelbrotPoints.encodePayload(
        EMPTY_POINTS_U16,
        1000,
      );

    const tx = await mirror.sendMessage(payload);
    const receipt = await tx.sendAndWaitForReceipt();

    expect(receipt.status).toBe("success");

    const { waitForReply } = await tx.setupReplyListener();
    const reply = await waitForReply();

    // The contract sends a separate message to the caller but does not create
    // an explicit reply payload for this handle, so Vara-Eth returns auto success.
    expectAutoSuccessReply(reply);
  });

  test("should check mandelbrot points via injected transaction", async () => {
    const payload =
      sails.services.MandelbrotChecker.functions.CheckMandelbrotPoints.encodePayload(
        EMPTY_POINTS_U16,
        1000,
      );

    const injected = await varaEthApi.createInjectedTransaction({
      destination: checkerId,
      payload,
      value: 0n,
    });

    const reply = await injected.sendAndWaitForPromise();

    expectAutoSuccessPromise(reply);
  });
});
