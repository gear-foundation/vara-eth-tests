import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { createVaraEthApi, WsVaraEthProvider } from "@vara-eth/api";
import { walletClientToSigner } from "@vara-eth/api/signer";
import {
  createPublicClient,
  createWalletClient,
  formatUnits,
  webSocket,
} from "viem";
import { privateKeyToAccount, nonceManager } from "viem/accounts";

const mode = process.argv[2];
const snapshotPath =
  process.env.TESTNET_COST_SNAPSHOT_PATH || ".testnet-cost-before.json";

if (mode !== "before" && mode !== "after") {
  throw new Error("Usage: node script/measure-testnet-costs.mjs <before|after>");
}

function readEnvFile(path) {
  if (!existsSync(path)) {
    throw new Error(`${path} does not exist`);
  }

  const entries = {};
  for (const line of readFileSync(path, "utf8").split("\n")) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#")) {
      continue;
    }

    const index = trimmed.indexOf("=");
    if (index === -1) {
      continue;
    }

    entries[trimmed.slice(0, index)] = trimmed.slice(index + 1);
  }

  return entries;
}

function requireEnv(env, key) {
  const value = env[key] || process.env[key];
  if (!value) {
    throw new Error(`${key} is required for test cost measurement`);
  }

  return value;
}

function formatDelta(value, decimals) {
  const sign = value < 0n ? "-" : "";
  const absolute = value < 0n ? -value : value;
  return `${sign}${formatUnits(absolute, decimals)}`;
}

async function readBalances() {
  const env = readEnvFile(".env");
  const ethereumRpc = requireEnv(env, "ETHEREUM_RPC");
  const varaEthRpc = requireEnv(env, "VARA_ETH_RPC");
  const router = requireEnv(env, "ROUTER_ADDRESS");
  const privateKey = requireEnv(env, "PRIVATE_KEY");

  const account = privateKeyToAccount(privateKey, { nonceManager });
  const publicClient = createPublicClient({
    transport: webSocket(ethereumRpc),
  });
  const walletClient = createWalletClient({
    account,
    transport: webSocket(ethereumRpc),
  });
  const varaEthApi = await createVaraEthApi(
    new WsVaraEthProvider(varaEthRpc),
    publicClient,
    router,
    walletClientToSigner(walletClient),
  );
  const address = await varaEthApi.eth.signer.getAddress();
  const [eth, wvara] = await Promise.all([
    publicClient.getBalance({ address }),
    varaEthApi.eth.wvara.balanceOf(address),
  ]);

  return {
    address,
    eth: eth.toString(),
    wvara: wvara.toString(),
  };
}

const balances = await readBalances();

if (mode === "before") {
  writeFileSync(snapshotPath, `${JSON.stringify(balances, null, 2)}\n`);
  console.log(
    `TEST_COSTS before eth=${formatUnits(BigInt(balances.eth), 18)} wvara=${formatUnits(BigInt(balances.wvara), 12)}`,
  );
} else {
  if (!existsSync(snapshotPath)) {
    throw new Error(`Missing cost snapshot: ${snapshotPath}`);
  }

  const before = JSON.parse(readFileSync(snapshotPath, "utf8"));
  const ethDelta = BigInt(before.eth) - BigInt(balances.eth);
  const wvaraDelta = BigInt(before.wvara) - BigInt(balances.wvara);

  console.log(
    `TEST_COSTS after eth=${formatUnits(BigInt(balances.eth), 18)} wvara=${formatUnits(BigInt(balances.wvara), 12)}`,
  );
  console.log(
    `TEST_COSTS delta eth=${formatDelta(ethDelta, 18)} wvara=${formatDelta(wvaraDelta, 12)}`,
  );
}
