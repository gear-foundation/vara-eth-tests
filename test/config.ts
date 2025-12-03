import dotenv from "dotenv";
import { Hex } from "viem";

dotenv.config({ quiet: true });

export const CONFIG = {
  privateKey: process.env.PRIVATE_KEY! as Hex,
  eth: {
    rpc: process.env.ETHEREUM_RPC!,
    router: process.env.ROUTER_ADDRESS! as Hex,
    blockTime: Number(process.env.BLOCK_TIME || "12"),
  },
  varaEth: {
    rpc: process.env.VARA_ETH_RPC! as "ws://",
  },
  artifactsDir: "./tmp",
};
