import { describe, test, expect } from "vitest";
import { publicClient, ethereumClient } from "./common";

describe("check balances", () => {
  test("check ETH balance", async () => {
    const balance = await publicClient.getBalance({
      address: ethereumClient.accountAddress,
    });

    expect(balance).toBeGreaterThan(0);
    console.log(`Account balance is ${balance / BigInt(1e18)} ETH`);
  });

  test("check WVARA balance", async () => {
    console.log(ethereumClient)
    const balance = await ethereumClient.wvara.balanceOf(
      ethereumClient.accountAddress,
    );

    expect(balance).toBeGreaterThan(1000n * BigInt(1e12));
    console.log(`Account balance is ${balance / BigInt(1e12)} WVARA`);
  });
});
