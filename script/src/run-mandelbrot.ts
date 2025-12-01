import 'dotenv/config';

import { createPublicClient, createWalletClient, http, hexToBytes } from 'viem';
import { privateKeyToAccount } from 'viem/accounts';
import { readFile, writeFile } from 'node:fs/promises';
import { InjectedTransaction, VaraEthApi, HttpVaraEthProvider, EthereumClient, getRouterClient, getWrappedVaraClient, getMirrorClient } from '@vara-eth/api';

import { Sails } from 'sails-js';
import { SailsIdlParser } from 'sails-js-parser';

const ETH_RPC = process.env.ETH_RPC!;
const PRIVATE_KEY = process.env.PRIVATE_KEY as `0x${string}`;
const ROUTER_ADDRESS = process.env.ROUTER_ADDRESS as `0x${string}`;
const VARA_HTTP = (
  process.env.VARA_HTTP ?? 'http://vara-eth-validator-1.gear-tech.io:9944'
) as `http://${string}` | `https://${string}`;

const CODE_ID = process.env.CODE_ID as `0x${string}`;       // checker code
const MAN_CODE_ID = process.env.MAN_CODE_ID as `0x${string}`; // manager code

const CHECKERS_PATH = new URL('../checker-programs.json', import.meta.url);

const MAN_IDL_PATH = new URL('../../contracts/target/wasm32-gear/release/manager.idl', import.meta.url);

const PROGRAM_COUNT = Number(process.env.PROGRAM_COUNT ?? '16');

type ProgramId = `0x${string}`;

type Mode = 'create-checkers' | 'run-manager' | 'full';
const MODE: Mode = (process.env.MODE as Mode) ?? 'full';

function addressToU16x32(addr: ProgramId): number[] {
  const bytes = hexToBytes(addr);

  if (bytes.length !== 20) {
    throw new Error(`Unexpected address length: ${bytes.length}, expected 20`);
  }

  const full32 = new Uint8Array(32);
  full32.set(bytes, 12);

  const arrU16 = Array.from(full32, (b) => b as number);

  if (arrU16.length !== 32) {
    throw new Error('Logic error: arrU16 must be length 32');
  }

  return arrU16;
}

async function waitForProgramOnVara(
  api: VaraEthApi,
  programId: ProgramId,
  {
    maxAttempts = 1000,
    delayMs = 3_000,
  }: { maxAttempts?: number; delayMs?: number } = {},
): Promise<void> {
  const target = programId.toLowerCase();

  for (let i = 0; i < maxAttempts; i++) {
    const ids = await api.query.program.getIds();
    const hasProgram = ids.map((x) => x.toLowerCase()).includes(target);

    if (hasProgram) {
      console.log(`Program ${programId} appeared on Vara.Eth (attempt ${i + 1}).`);
      return;
    }

    console.log(
      `Program not yet visible on Vara.Eth, attempt ${i + 1}/${maxAttempts}...`,
    );
    await new Promise((resolve) => setTimeout(resolve, delayMs));
  }

  throw new Error(`Program ${programId} did not appear on Vara.Eth in time`);
}


async function initClients() {
  if (!ETH_RPC) throw new Error('ETH_RPC is not set');
  if (!PRIVATE_KEY) throw new Error('PRIVATE_KEY is not set');
  if (!ROUTER_ADDRESS) throw new Error('ROUTER_ADDRESS is not set');

  console.log('Using ETH RPC:', ETH_RPC);
  console.log('Router:', ROUTER_ADDRESS);
  console.log('Vara HTTP:', VARA_HTTP);

  const publicClient = createPublicClient({
    transport: http(ETH_RPC),
  });

  const account = privateKeyToAccount(PRIVATE_KEY);

  const walletClient = createWalletClient({
    account,
    transport: http(ETH_RPC),
  });

  const ethereumClient = new EthereumClient(publicClient, walletClient);

  const api = new VaraEthApi(
    new HttpVaraEthProvider(VARA_HTTP),
    ethereumClient,
    ROUTER_ADDRESS,
  );

  const router = getRouterClient(ROUTER_ADDRESS, ethereumClient);
  const wvara = getWrappedVaraClient(await router.wrappedVara(), ethereumClient);

  return { ethereumClient, api, router, wvara };
}

async function createCheckerPrograms(
  ethereumClient: EthereumClient,
  api: VaraEthApi,
  router: ReturnType<typeof getRouterClient>,
  wvara: ReturnType<typeof getWrappedVaraClient>,
): Promise<ProgramId[]> {
  if (!CODE_ID) throw new Error('CODE_ID is not set');

  console.log('CodeId (checker):', CODE_ID);

  const topUpAmount = BigInt(1000 * 1e12);
  const balance = await wvara.balanceOf(ethereumClient.accountAddress);
  console.log('Sender wVARA balance:', balance.toString());

  const programIds: ProgramId[] = [];

  for (let i = 0; i < PROGRAM_COUNT; i++) {
    console.log(`\n[${i + 1}/${PROGRAM_COUNT}] Creating checker program from codeId...`);

    const tx = await router.createProgram(CODE_ID);
    const receipt = await tx.sendAndWaitForReceipt();
    console.log('  Tx sent. Hash:', receipt.transactionHash);

    const programId = (await tx.getProgramId()) as ProgramId;
    console.log('  New checker programId:', programId);

    programIds.push(programId);

    const mirror = getMirrorClient(programId, ethereumClient);

    await waitForProgramOnVara(api, programId);

    const approveTx = await wvara.approve(programId, topUpAmount);
    await approveTx.sendAndWaitForReceipt();

    const topUpTx = await mirror.executableBalanceTopUp(topUpAmount);
    await topUpTx.sendAndWaitForReceipt();

    // init()
    const msgTx = await mirror.sendMessage('0x0c4e6577', 0n);
    await msgTx.send();
    const { waitForReply } = await msgTx.setupReplyListener();
    await waitForReply();

    const stateHash = await mirror.stateHash();
    const state = await api.query.program.readState(stateHash);

    console.log('  Program status:', state.program);
    console.log('  Executable balance:', state.balance);
  }

  console.log('\nAll checker programIds:');
  console.log(programIds);

  return programIds;
}

async function saveCheckerPrograms(programIds: ProgramId[]) {
  await writeFile(CHECKERS_PATH, JSON.stringify(programIds, null, 2), 'utf8');
  console.log(`Checker program IDs saved to: ${CHECKERS_PATH.toString()}`);
}

async function loadCheckerPrograms(): Promise<ProgramId[]> {

  const content = await readFile(CHECKERS_PATH, 'utf8');
  const fromFile = JSON.parse(content) as string[];

  const programs = fromFile.map((p) => p as ProgramId);
  console.log('Loaded checker programs from file:', CHECKERS_PATH.toString());
  console.log(programs);

  if (!programs.length) {
    throw new Error('No checker programs found in file');
  }

  return programs;
}

async function initSails(programId: ProgramId) {
  const parser = await SailsIdlParser.new();
  const sails = new Sails(parser);

  const idl = await readFile(MAN_IDL_PATH, 'utf8');

  sails.parseIdl(idl);
  sails.setProgramId(programId);

  return sails;
}


async function sendManagerMessage(
  mirror: ReturnType<typeof getMirrorClient>,
  payload: string,
  value: bigint = 0n,
): Promise<void> {
  const msgTx = await mirror.sendMessage(payload as `0x${string}`, value);
  await msgTx.send();

  const { waitForReply } = await msgTx.setupReplyListener();
  const reply = await waitForReply();

  console.log('Reply:', reply.payload, reply.replyCode, reply.value);
}

async function sendInjectedMessage(
  programId: ProgramId,
  api: VaraEthApi,
  payload: string,
): Promise<void> {
  const tx = new InjectedTransaction({
    destination: programId,
    payload: payload as `0x${string}`,
    value: 0n,
  });

  const result = await (await api.createInjectedTransaction(tx)).send();
  console.log('Injected tx result:', result); 
}

async function getPointsLen(
  sails: Sails,
  api: VaraEthApi,
  ethereumClient: EthereumClient,
  programId: ProgramId,
): Promise<number> {
  const queryPayload = sails.services.Manager.queries.GetPointsLen.encodePayload();

  const queryReply = await api.call.program.calculateReplyForHandle(
    ethereumClient.accountAddress,
    programId,
    queryPayload as `0x${string}`,
  );

  const decoded = sails.services.Manager.queries.GetPointsLen.decodeResult(
    queryReply.payload,
  );

  console.log('Amount of points:', decoded);
  return decoded;
}

async function getCheckedCount(
  sails: Sails,
  api: VaraEthApi,
  ethereumClient: EthereumClient,
  programId: ProgramId,
): Promise<number> {
  const queryPayload = sails.services.Manager.queries.GetCheckedCount.encodePayload();

  const queryReply = await api.call.program.calculateReplyForHandle(
    ethereumClient.accountAddress,
    programId,
    queryPayload as `0x${string}`,
  );

  const decoded = sails.services.Manager.queries.GetCheckedCount.decodeResult(
    queryReply.payload,
  );

  console.log('Amount of checked points:', decoded);
  return decoded;
}

async function getPointsSent(
  sails: Sails,
  api: VaraEthApi,
  ethereumClient: EthereumClient,
  programId: ProgramId,
): Promise<number> {
  const queryPayload = sails.services.Manager.queries.PointsSent.encodePayload();

  const queryReply = await api.call.program.calculateReplyForHandle(
    ethereumClient.accountAddress,
    programId,
    queryPayload as `0x${string}`,
  );

  const decoded = sails.services.Manager.queries.PointsSent.decodeResult(
    queryReply.payload,
  );

  console.log('Amount of sent points:', decoded);
  return decoded;
}

async function runManagerFlow(
  ethereumClient: EthereumClient,
  api: VaraEthApi,
  router: ReturnType<typeof getRouterClient>,
  wvara: ReturnType<typeof getWrappedVaraClient>,
  checkerPrograms: ProgramId[],
): Promise<void> {
  if (!MAN_CODE_ID) throw new Error('MAN_CODE_ID is not set');

  console.log('Manager CodeId:', MAN_CODE_ID);
  console.log('Using checker programs:', checkerPrograms);

  const topUpAmount = BigInt(1000 * 1e12);

  const tx = await router.createProgram(MAN_CODE_ID);
  await tx.sendAndWaitForReceipt();

  const programId = (await tx.getProgramId()) as ProgramId;
  console.log('Manager programId:', programId);

  await waitForProgramOnVara(api, programId);

  const mirror = getMirrorClient(programId, ethereumClient);

  const approveTx = await wvara.approve(programId, topUpAmount);
  await approveTx.sendAndWaitForReceipt();

  const topUpTx = await mirror.executableBalanceTopUp(topUpAmount);
  const { status } = await topUpTx.sendAndWaitForReceipt();
  console.log('Executable balance result:', status);

  // init()
  const initMsgTx = await mirror.sendMessage('0x0c4e6577', 0n);
  await initMsgTx.send();
  const { waitForReply: initReply } = await initMsgTx.setupReplyListener();
  const reply = await initReply();
  console.log('Init reply:', reply);

  const stateHash = await mirror.stateHash();
  const state = await api.query.program.readState(stateHash);
  const balance = await wvara.balanceOf(programId);

  console.log('Program status:', state.program);
  console.log('Executable balance:', state.executableBalance);
  console.log('Balance:', balance.toString());

  const sails = await initSails(programId);

  // GenerateAndStorePoints
  const genPointsPayloadBytes =
    sails.services.Manager.functions.GenerateAndStorePoints.encodePayload(
      400,  // width
      400,  // height
      -2,   // x_min
      0,    // x_min_frac
      1,    // x_max
      0,    // x_max_frac
      -15,  // y_min
      1,    // y_min_frac
      15,   // y_max
      1,    // y_max_frac
      30000, // max_iter
      true,  
      false, 
      0,    
      0,    
    );

  await sendManagerMessage(mirror, genPointsPayloadBytes as `0x${string}`, 0n);
  await getPointsLen(sails, api, ethereumClient, programId);

  // Add checkers
  const checkerInput: number[][] = checkerPrograms.map(addressToU16x32);
  const checkersPayloadBytes =
    sails.services.Manager.functions.AddCheckers.encodePayload(checkerInput);

  await sendManagerMessage(mirror, checkersPayloadBytes as `0x${string}`, 0n);

  // caculate 
  const checkPayloadBytes =
    sails.services.Manager.functions.CheckPointsSet.encodePayload(1000, 40, 800);

  await sendManagerMessage(mirror, checkPayloadBytes);
  await getCheckedCount(sails, api, ethereumClient, programId);
  await getPointsSent(sails, api, ethereumClient, programId);

  console.log('Manager flow finished');
}

async function main() {
  console.log('MODE =', MODE);

  const { ethereumClient, api, router, wvara } = await initClients();

  if (MODE === 'create-checkers') {
    const checkers = await createCheckerPrograms(ethereumClient, api, router, wvara);
    await saveCheckerPrograms(checkers);
    return;
  }

  if (MODE === 'run-manager') {
    const checkerPrograms = await loadCheckerPrograms();
    await runManagerFlow(ethereumClient, api, router, wvara, checkerPrograms);
    return;
  }

  // MODE === 'full'
  const checkers = await createCheckerPrograms(ethereumClient, api, router, wvara);
  await saveCheckerPrograms(checkers);
  await runManagerFlow(ethereumClient, api, router, wvara, checkers);
}

main()
  .then(() => {
    console.log('Done');
    process.exit(0);
  })
  .catch((err) => {
    console.error('Error:', err);
    process.exit(1);
  });