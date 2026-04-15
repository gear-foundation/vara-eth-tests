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
  expectAutoSuccessReply,
  expectManualSuccessPromise,
  expectManualSuccessReply,
} from "../helpers/replies";

const IDL_PATH = "./artifacts/idl/extended_vft.idl";
const idlContent = readFileSync(IDL_PATH, "utf-8");

const TOP_UP_AMOUNT = BigInt(100 * 1e12);
const MAX_WAIT_BLOCKS = 30;
const LOAD_CONFIG = {
  readConcurrency: Number(process.env.LOAD_READ_CONCURRENCY || "5"),
  readIterations: Number(process.env.LOAD_READ_ITERATIONS || "3"),
  mirrorBurstSize: Number(process.env.LOAD_MIRROR_BURST_SIZE || "3"),
  injectedBurstSize: Number(process.env.LOAD_INJECTED_BURST_SIZE || "3"),
};
const INITIAL_MINT_AMOUNT = "1000000000000";
const BURST_MINT_AMOUNT = "100000000000";

let vftId: Hex;
let mirror: MirrorClient;
let stateHash: Hex;
let mirrorBurstDelta = 0n;
let injectedBurstDelta = 0n;

function paddedActor(address: Hex) {
  return `0x${address.slice(2).padStart(64, "0")}`;
}

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

async function readBalanceOf(account: Hex) {
  const payload = sails.services.Vft.queries.BalanceOf.encodePayload(
    paddedActor(account),
  );
  const reply = await varaEthApi.call.program.calculateReplyForHandle(
    accountAddress,
    vftId,
    payload as `0x${string}`,
  );

  return BigInt(sails.services.Vft.queries.BalanceOf.decodeResult(reply.payload));
}

async function readTotalSupply() {
  const payload = sails.services.Vft.queries.TotalSupply.encodePayload();
  const reply = await varaEthApi.call.program.calculateReplyForHandle(
    accountAddress,
    vftId,
    payload as `0x${string}`,
  );

  return BigInt(sails.services.Vft.queries.TotalSupply.decodeResult(reply.payload));
}

async function createInjectedMintTransaction(payload: Hex, index: number) {
  const injected = await varaEthApi.createInjectedTransaction({
    destination: vftId,
    payload,
    value: 0n,
  });

  const validatorMode = process.env.VFT_INJECTED_VALIDATOR_MODE || "default";
  const recipient =
    validatorMode === "slot"
      ? await injected.setSlotValidator()
      : injected.setDefaultValidator();

  console.log(
    `[vft-load-injected-${index}] Prepared injected transaction`,
    {
      validatorMode,
      recipient,
      messageId: injected.messageId,
      txHash: injected.txHash,
      referenceBlock: injected.referenceBlock,
    },
  );

  return injected;
}

async function waitForExpectedTokenState(
  expectedBalance: bigint,
  expectedTotalSupply: bigint,
) {
  for (let i = 0; i < MAX_WAIT_BLOCKS; i++) {
    const [balance, totalSupply] = await Promise.all([
      readBalanceOf(accountAddress),
      readTotalSupply(),
    ]);

    if (balance === expectedBalance && totalSupply === expectedTotalSupply) {
      return;
    }

    await wait1Block();
  }

  throw new Error(
    `Expected balance=${expectedBalance} and totalSupply=${expectedTotalSupply} were not observed within ${MAX_WAIT_BLOCKS} blocks`,
  );
}

describe("vft load", () => {
  const codeId = process.env.TOKEN_ID as Hex;

  test("should prepare vft for load scenarios", async () => {
    expect(codeId).toBeDefined();
    expect(codeId).toHaveLength(66);

    const tx = await ethereumClient.router.createProgram(codeId);
    const receipt = await tx.sendAndWaitForReceipt();

    expect(receipt.status).toBe("success");

    vftId = await tx.getProgramId();
    expect(vftId).toBeDefined();

    mirror = getMirrorClient({
      address: vftId,
      publicClient,
      signer: ethereumClient.signer,
    });

    await waitForProgramOnVaraEth(vftId);

    const approveTx = await ethereumClient.wvara.approve(vftId, TOP_UP_AMOUNT);
    const approveReceipt = await approveTx.sendAndWaitForReceipt();
    expect(approveReceipt.status).toBe("success");

    const topUpTx = await mirror.executableBalanceTopUp(TOP_UP_AMOUNT);
    const topUpReceipt = await topUpTx.sendAndWaitForReceipt();
    expect(topUpReceipt.status).toBe("success");

    stateHash = await mirror.stateHash();

    sails.parseIdl(idlContent);

    const initTx = await mirror.sendMessage(
      sails.ctors.Init.encodePayload("LoadToken", "LOAD", "12"),
    );
    const initReceipt = await initTx.sendAndWaitForReceipt();
    expect(initReceipt.status).toBe("success");

    const { waitForReply } = await initTx.setupReplyListener();
    const initReply = await waitForReply();
    expectAutoSuccessReply(initReply);

    const mintTx = await mirror.sendMessage(
      sails.services.Vft.functions.Mint.encodePayload(
        paddedActor(accountAddress),
        INITIAL_MINT_AMOUNT,
      ),
    );
    const mintReceipt = await mintTx.sendAndWaitForReceipt();
    expect(mintReceipt.status).toBe("success");

    const mintReplyWaiter = await mintTx.setupReplyListener();
    const mintReply = await mintReplyWaiter.waitForReply();
    expectManualSuccessReply(mintReply);

    const minted = sails.services.Vft.functions.Mint.decodeResult(mintReply.payload);
    expect(minted).toBe(true);

    await waitForExpectedTokenState(
      BigInt(INITIAL_MINT_AMOUNT),
      BigInt(INITIAL_MINT_AMOUNT),
    );
  });

  test("should handle parallel read load", async () => {
    const expectedBalance = BigInt(INITIAL_MINT_AMOUNT);
    const expectedTotalSupply = BigInt(INITIAL_MINT_AMOUNT);
    const latencies: number[] = [];

    for (let iteration = 0; iteration < LOAD_CONFIG.readIterations; iteration++) {
      const batch = Array.from({ length: LOAD_CONFIG.readConcurrency }, async () => {
        const startedAt = performance.now();
        const [balance, totalSupply, currentStateHash] = await Promise.all([
          readBalanceOf(accountAddress),
          readTotalSupply(),
          mirror.stateHash(),
        ]);
        const state = await varaEthApi.query.program.readState(currentStateHash);
        const elapsed = performance.now() - startedAt;

        latencies.push(elapsed);
        expect(balance).toBe(expectedBalance);
        expect(totalSupply).toBe(expectedTotalSupply);
        expect(BigInt(state.executableBalance)).toBeGreaterThan(0n);
      });

      await Promise.all(batch);
    }

    expect(latencies).toHaveLength(
      LOAD_CONFIG.readConcurrency * LOAD_CONFIG.readIterations,
    );
    logLatencyMetrics("vft-parallel-read-load", latencies);
  });

  test("should handle mirror write burst", async () => {
    const amount = BigInt(BURST_MINT_AMOUNT);
    const burstSize = BigInt(LOAD_CONFIG.mirrorBurstSize);
    const balanceBefore = await readBalanceOf(accountAddress);
    const totalSupplyBefore = await readTotalSupply();
    const prevStateHash = await mirror.stateHash();
    const payload = sails.services.Vft.functions.Mint.encodePayload(
      paddedActor(accountAddress),
      BURST_MINT_AMOUNT,
    );
    const startingNonce = await publicClient.getTransactionCount({
      address: accountAddress,
      blockTag: "pending",
    });

    const txs = await Promise.all(
      Array.from({ length: LOAD_CONFIG.mirrorBurstSize }, () =>
        mirror.sendMessage(payload),
      ),
    );

    txs.forEach((tx, index) => {
      tx.getTx().nonce = startingNonce + index;
    });

    const receipts = await Promise.all(
      txs.map((tx) => tx.sendAndWaitForReceipt()),
    );

    for (const receipt of receipts) {
      expect(receipt.status).toBe("success");
    }

    const replyListeners = await Promise.all(
      txs.map((tx) => tx.setupReplyListener()),
    );
    const replies = await Promise.all(
      replyListeners.map(({ waitForReply }) => waitForReply()),
    );

    for (const reply of replies) {
      expectManualSuccessReply(reply);

      const result = sails.services.Vft.functions.Mint.decodeResult(reply.payload);
      expect(result).toBe(true);
    }

    stateHash = await waitForStateHashChange(prevStateHash);

    const expectedBalance = balanceBefore + amount * burstSize;
    const expectedTotalSupply = totalSupplyBefore + amount * burstSize;
    await waitForExpectedTokenState(expectedBalance, expectedTotalSupply);

    const balanceAfter = await readBalanceOf(accountAddress);
    const totalSupplyAfter = await readTotalSupply();

    expect(balanceAfter).toBe(expectedBalance);
    expect(totalSupplyAfter).toBe(expectedTotalSupply);

    mirrorBurstDelta = totalSupplyAfter - totalSupplyBefore;
  });

  test("should handle injected write burst", async () => {
    const amount = BigInt(BURST_MINT_AMOUNT);
    const burstSize = BigInt(LOAD_CONFIG.injectedBurstSize);
    const balanceBefore = await readBalanceOf(accountAddress);
    const totalSupplyBefore = await readTotalSupply();
    const prevStateHash = await mirror.stateHash();
    const payload = sails.services.Vft.functions.Mint.encodePayload(
      paddedActor(accountAddress),
      BURST_MINT_AMOUNT,
    );

    const replies = await Promise.all(
      Array.from({ length: LOAD_CONFIG.injectedBurstSize }, async (_, index) => {
        const injected = await createInjectedMintTransaction(payload, index);
        return injected.sendAndWaitForPromise();
      }),
    );

    for (const reply of replies) {
      expectManualSuccessPromise(reply);
      const result = sails.services.Vft.functions.Mint.decodeResult(reply.payload);
      expect(result).toBe(true);
    }

    stateHash = await waitForStateHashChange(prevStateHash);

    const expectedBalance = balanceBefore + amount * burstSize;
    const expectedTotalSupply = totalSupplyBefore + amount * burstSize;
    await waitForExpectedTokenState(expectedBalance, expectedTotalSupply);

    const balanceAfter = await readBalanceOf(accountAddress);
    const totalSupplyAfter = await readTotalSupply();

    expect(balanceAfter).toBe(expectedBalance);
    expect(totalSupplyAfter).toBe(expectedTotalSupply);

    injectedBurstDelta = totalSupplyAfter - totalSupplyBefore;
  });

  test("should keep mirror and injected business deltas in parity", () => {
    expect(mirrorBurstDelta).toBe(
      BigInt(LOAD_CONFIG.mirrorBurstSize) * BigInt(BURST_MINT_AMOUNT),
    );
    expect(injectedBurstDelta).toBe(
      BigInt(LOAD_CONFIG.injectedBurstSize) * BigInt(BURST_MINT_AMOUNT),
    );

    const mirrorPerOperation =
      mirrorBurstDelta / BigInt(LOAD_CONFIG.mirrorBurstSize || 1);
    const injectedPerOperation =
      injectedBurstDelta / BigInt(LOAD_CONFIG.injectedBurstSize || 1);

    expect(mirrorPerOperation).toBe(BigInt(BURST_MINT_AMOUNT));
    expect(injectedPerOperation).toBe(BigInt(BURST_MINT_AMOUNT));
    expect(mirrorPerOperation).toBe(injectedPerOperation);
  });
});
