import { EthereumClient, VaraEthApi, WsVaraEthProvider } from "@vara-eth/api";
import {
  Chain,
  createPublicClient,
  createWalletClient,
  PublicClient,
  WalletClient,
  webSocket,
  WebSocketTransport,
} from "viem";
import { CONFIG } from "./config";
import { Account, privateKeyToAccount, nonceManager } from "viem/accounts";
import { Sails } from "sails-js";
import { SailsIdlParser } from "sails-js-parser";

export let publicClient: PublicClient<WebSocketTransport, Chain>;
export let walletClient: WalletClient<WebSocketTransport, Chain, Account>;
export let varaEthApi: VaraEthApi;
export let ethereumClient: EthereumClient;
export let sails: Sails;

export async function init() {
  const transport = webSocket(CONFIG.eth.rpc);
  const account = privateKeyToAccount(CONFIG.privateKey, { nonceManager });

  publicClient = createPublicClient({ transport });
  walletClient = createWalletClient({ transport, account });

  ethereumClient = new EthereumClient(
    publicClient,
    walletClient,
    CONFIG.eth.router,
  );

  console.log(CONFIG.varaEth.rpc)
  const provider = new WsVaraEthProvider(CONFIG.varaEth.rpc);
  varaEthApi = new VaraEthApi(provider, ethereumClient);

  const parser = await SailsIdlParser.new();
  sails = new Sails(parser);
}

export const wait1Block = () =>
  new Promise((resolve) => setTimeout(resolve, CONFIG.eth.blockTime * 1_000));
