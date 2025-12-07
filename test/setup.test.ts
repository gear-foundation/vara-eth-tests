import { describe, test, expect } from "vitest";

import { CONFIG } from "./config";
import {
  init,
  publicClient,
  walletClient,
  ethereumClient,
  varaEthApi,
  sails,
} from "./common";

describe("environment variables", () => {
  test("PRIVATE_KEY provided", () => {
    expect(CONFIG.privateKey).toBeDefined();
    expect(CONFIG.privateKey.startsWith("0x")).toBeTruthy();
    expect(CONFIG.privateKey).toHaveLength(66);
  });

  test("ETHEREUM_RPC provided", () => {
    expect(CONFIG.eth.rpc).toBeDefined();
    expect(
      CONFIG.eth.rpc.startsWith("wss://") || CONFIG.eth.rpc.startsWith("ws://"),
    ).toBeTruthy();
  });

  test("ROUTER_ADDRESS provided", () => {
    expect(CONFIG.eth.router).toBeDefined();
    expect(CONFIG.eth.router.startsWith("0x")).toBeTruthy();
    expect(CONFIG.eth.router).toHaveLength(42);
  });

  test("VARA_ETH_RPC provided", () => {
    expect(CONFIG.varaEth.rpc).toBeDefined();
    expect(
      CONFIG.varaEth.rpc.startsWith("wss://") ||
        CONFIG.varaEth.rpc.startsWith("ws://"),
    ).toBeTruthy();
  });
});

describe("setup connections", () => {
  test("should initialize connections", async () => {
    await init();

    expect(publicClient).toBeDefined();
    expect(walletClient).toBeDefined();
    expect(ethereumClient).toBeDefined();
    expect(varaEthApi).toBeDefined();
    expect(sails).toBeDefined();
  });
});
