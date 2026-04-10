import { getMirrorClient, MirrorClient } from "@vara-eth/api";
import { readFileSync } from "node:fs";
import { performance } from "node:perf_hooks";
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

const IDL_PATH = "./target/wasm32-gear/release/mandelbrot_checker.idl";
const idlContent = readFileSync(IDL_PATH, "utf-8");

const TOP_UP_AMOUNT = BigInt(100 * 1e12);
const MAX_WAIT_BLOCKS = 30;
const EMPTY_POINTS_U16 = [0];

const LOAD_CONFIG = {
  readConcurrency: Number(process.env.LOAD_READ_CONCURRENCY || "10"),
  readIterations: Number(process.env.LOAD_READ_ITERATIONS || "5"),
  injectedBurstSize: Number(process.env.LOAD_INJECTED_BURST_SIZE || "5"),
};

let checkerId: Hex;
let mirror: MirrorClient;
let stateHash: Hex;

function percentile(values: number[], ratio: number) {
  const sorted = [...values].sort((a, b) => a - b);
  const index = Math.min(
    sorted.length - 1,
    Math.max(0, Math.ceil(sorted.length * ratio) - 1),
  );

  return sorted[index];
}

function logLatencyMetrics(label: string, values: number[]) {
  const total = values.reduce((sum, value) => sum + value, 0);
  const avg = total / values.length;

  console.log(
    `[${label}] count=${values.length} avg=${avg.toFixed(2)}ms p50=${percentile(values, 0.5).toFixed(2)}ms p95=${percentile(values, 0.95).toFixed(2)}ms max=${Math.max(...values).toFixed(2)}ms`,
  );
}

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

async function waitForStateHashChange(prevStateHash: Hex) {
  for (let i = 0; i < MAX_WAIT_BLOCKS; i++) {
    await wait1Block();

    const nextStateHash = await mirror.stateHash();
    if (nextStateHash !== prevStateHash) {
      return nextStateHash;
    }
  }

  throw new Error(
    `State hash did not change within ${MAX_WAIT_BLOCKS} blocks`,
  );
}

describe("mandelbrot checker load", () => {
  const codeId = process.env.CHECKER_CODE_ID! as Hex;

  test("should prepare checker for load scenarios", async () => {
    expect(codeId).toBeDefined();
    expect(codeId).toHaveLength(66);

    const tx = await ethereumClient.router.createProgram(codeId);
    const receipt = await tx.sendAndWaitForReceipt();

    expect(receipt.status).toBe("success");

    checkerId = await tx.getProgramId();
    expect(checkerId).toBeDefined();

    mirror = getMirrorClient({
      address: checkerId,
      publicClient,
      signer: ethereumClient.signer,
    });

    await waitForProgramOnVaraEth(checkerId);

    const approveTx = await ethereumClient.wvara.approve(checkerId, TOP_UP_AMOUNT);
    const approveReceipt = await approveTx.sendAndWaitForReceipt();
    expect(approveReceipt.status).toBe("success");

    const topUpTx = await mirror.executableBalanceTopUp(TOP_UP_AMOUNT);
    const topUpReceipt = await topUpTx.sendAndWaitForReceipt();
    expect(topUpReceipt.status).toBe("success");

    stateHash = await mirror.stateHash();

    sails.parseIdl(idlContent);

    const initTx = await mirror.sendMessage(sails.ctors.Init.encodePayload());
    const initReceipt = await initTx.sendAndWaitForReceipt();
    expect(initReceipt.status).toBe("success");

    const { waitForReply } = await initTx.setupReplyListener();
    const initReply = await waitForReply();
    expectAutoSuccessReply(initReply);
  });

  test("should handle parallel read load", async () => {
    const latencies: number[] = [];
    const totalRequests =
      LOAD_CONFIG.readConcurrency * LOAD_CONFIG.readIterations;

    const tasks = Array.from({ length: totalRequests }, async () => {
      const startedAt = performance.now();
      const currentStateHash = await mirror.stateHash();
      const state = await varaEthApi.query.program.readState(currentStateHash);
      const elapsed = performance.now() - startedAt;

      latencies.push(elapsed);
      expect(BigInt(state.executableBalance)).toBeGreaterThan(0n);
    });

    await Promise.all(tasks);

    expect(latencies).toHaveLength(totalRequests);
    logLatencyMetrics("parallel-read-load", latencies);
  });

  test("should handle injected transaction burst load", async () => {
    const payload =
      sails.services.MandelbrotChecker.functions.CheckMandelbrotPoints.encodePayload(
        EMPTY_POINTS_U16,
        1000,
      );

    const previousStateHash = await mirror.stateHash();
    const startedAt = performance.now();

    const replies = await Promise.all(
      Array.from({ length: LOAD_CONFIG.injectedBurstSize }, async () => {
        const injected = await varaEthApi.createInjectedTransaction({
          destination: checkerId,
          payload,
          value: 0n,
        });

        return injected.sendAndWaitForPromise();
      }),
    );

    const elapsed = performance.now() - startedAt;

    for (const reply of replies) {
      expectAutoSuccessPromise(reply);
    }

    stateHash = await waitForStateHashChange(previousStateHash);

    console.log(
      `[injected-burst-load] count=${LOAD_CONFIG.injectedBurstSize} total=${elapsed.toFixed(2)}ms avg=${(elapsed / LOAD_CONFIG.injectedBurstSize).toFixed(2)}ms`,
    );
  });
});
