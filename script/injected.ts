import 'dotenv/config';

import { createPublicClient, createWalletClient, hexToBytes, webSocket } from 'viem';
import { privateKeyToAccount } from 'viem/accounts';
import { readFile, writeFile } from 'node:fs/promises';
import { VaraEthApi, WsVaraEthProvider, EthereumClient, getMirrorClient, getRouterClient, getWrappedVaraClient } from '@vara-eth/api';
import type { Hex } from "viem";
import type { IInjectedTransaction } from '@vara-eth/api';
import { Sails } from 'sails-js';
import { SailsIdlParser } from 'sails-js-parser';
type ProgramId = `0x${string}`;

const IDL_PATH = new URL('./digit_recognition.idl', import.meta.url);
const ETHEREUM_RPC = 'wss://hoodi-reth-rpc.gear-tech.io/ws';
const PRIVATE_KEY = '0x58106437f85ca382e216e36318aec10ed47cc05f40cd6d7ea45f616d4a7c45f5';
const ROUTER_ADDRESS = '0xBC888a8B050B9B76a985d91c815d2c4f2131a58A';
const VARA_ETH_RPC = 'ws://vara-eth-validator-1.gear-tech.io:9944' as "ws://";
const PROGRAM_ID = '0xc8dc84313016fcc9fbe44731b8420f8edd3da234';

function normalizeHexPayload(raw: string): `0x${string}` {
  const s = raw.replace(/\s+/g, "").trim();
  if (!s.startsWith("0x")) throw new Error("Payload must start with 0x");
  const hex = s.slice(2);
  if (hex.length === 0) throw new Error("Payload is empty");
  if (hex.length % 2 !== 0) throw new Error(`Payload hex length must be even, got ${hex.length}`);
  if (!/^[0-9a-fA-F]+$/.test(hex)) throw new Error("Payload contains non-hex characters");
  return s as `0x${string}`;
}

type PayloadMap = Record<string, `0x${string}`>;
function parseSectionedPayloadFile(text: string): PayloadMap {
  const lines = text.split(/\r?\n/);

  const sections: Record<string, string[]> = {};
  let current: string | null = null;

  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#") || trimmed.startsWith("//")) continue;

    const m = trimmed.match(/^\[(.+)\]$/);
    if (m) {
      current = m[1].trim();
      if (!current) throw new Error("Empty section name []");
      if (!sections[current]) sections[current] = [];
      continue;
    }

    if (!current) {
      throw new Error(`Found data before any section header: ${trimmed.slice(0, 32)}...`);
    }
    sections[current].push(trimmed);
  }

  const out: PayloadMap = {};
  for (const [name, chunkLines] of Object.entries(sections)) {
    out[name] = normalizeHexPayload(chunkLines.join(""));
  }
  return out;
}

async function initSails(programId: ProgramId) {
  const parser = await SailsIdlParser.new();
  const sails = new Sails(parser);

  const idl = await readFile(IDL_PATH, 'utf8');

  sails.parseIdl(idl);
  sails.setProgramId(programId);

  return sails;
}


async function initClients() {
  if (!ETHEREUM_RPC) throw new Error('ETH_RPC is not set');
  if (!PRIVATE_KEY) throw new Error('PRIVATE_KEY is not set');
  if (!ROUTER_ADDRESS) throw new Error('ROUTER_ADDRESS is not set');

  console.log('Using ETH RPC:', ETHEREUM_RPC);
  console.log('Router:', ROUTER_ADDRESS);
  console.log('Vara HTTP:', VARA_ETH_RPC);

  const transport = webSocket(ETHEREUM_RPC);

  const publicClient = createPublicClient({ transport });

  const account = privateKeyToAccount(PRIVATE_KEY);

  const walletClient = createWalletClient({account,transport});

  const ethereumClient = new EthereumClient(publicClient, walletClient, ROUTER_ADDRESS);

  await ethereumClient.isInitialized;

  const api = new VaraEthApi(
    new WsVaraEthProvider(VARA_ETH_RPC),
    ethereumClient,
  );

  const router = ethereumClient.router;
  const wvara = ethereumClient.wvara;
  return { ethereumClient, api, router, wvara, walletClient, publicClient };
}

async function sendInjectedTx(
  api: VaraEthApi,
  programId: ProgramId,
  payload: `0x${string}`,
): Promise<void> {
    const injected: IInjectedTransaction = {
        destination: programId,
        payload,
      };
    const tx = await api.createInjectedTransaction(injected);

    // const promise = await injected.send();
    // console.log(promise)
    const promise = await tx.sendAndWaitForPromise();
    console.log(promise)
    console.log(promise.reply.code)
}

async function readRLayers(
  sails: Sails,
  api: VaraEthApi,
  ethereumClient: EthereumClient,
  programId: ProgramId,
): Promise<number> {
  const queryPayload = sails.services.DigitRecognition.queries.LayersSet.encodePayload();

  const queryReply = await api.call.program.calculateReplyForHandle(
    ethereumClient.accountAddress,
    programId,
    queryPayload as `0x${string}`,
  );

  const decoded = sails.services.DigitRecognition.queries.LayersSet.decodeResult(
    queryReply.payload,
  );

  console.log('Layers: ', decoded);
  return decoded;
}

async function main() {

  const { ethereumClient, api, router, wvara, walletClient, publicClient } = await initClients();
  const sails = await initSails(PROGRAM_ID);
  await readRLayers(sails, api, ethereumClient, PROGRAM_ID);
  const raw = await readFile(new URL("./payloads.txt", import.meta.url), "utf8");
  const payloads = parseSectionedPayloadFile(raw);
  await sendInjectedTx(api, PROGRAM_ID, payloads["fc1"]);
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