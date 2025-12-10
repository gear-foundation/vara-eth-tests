import { getMirrorClient, MirrorClient, IInjectedTransaction } from "@vara-eth/api";
import { Hex } from "viem";
import path from "node:path";

import {
  publicClient,
  walletClient,
  varaEthApi,
  ethereumClient,
  wait1Block,
  sails,
} from "../common";
import { readFileSync } from "node:fs";
import { CONFIG } from "../config";

let vftId: Hex;
//let vftId = '0x5564a4e90bf53ce713de30161602a4fd696d6e7e' as Hex;
let stateHash: Hex;

const IDL_PATH = "./target/wasm32-gear/release/extended_vft.idl";
const idlContent = readFileSync(IDL_PATH, "utf-8");
describe("create token", () => {
  const codeId = process.env.TOKEN_ID! as Hex;
  const TOP_UP_AMOUNT = BigInt(100 * 1e12);
  let mirror: MirrorClient;

  test("should check CODE_ID", () => {
    expect(codeId).toBeDefined();
    expect(codeId).toHaveLength(66);
    console.log(`Using code id ${codeId} for manager program`);
  });

  test("should create program", async () => {
    const tx = await ethereumClient.router.createProgram(codeId);

    const receipt = await tx.sendAndWaitForReceipt();

    expect(receipt.status).toBe("success");

    const programId = await tx.getProgramId();

    expect(programId).toBeDefined();
    vftId = programId;
    console.log("New manager program created:", programId);
  });

  test("should wait for program appeared on Vara.Eth", async () => {
    let appeared = false;

    while (!appeared) {
      await wait1Block();
      const ids = await varaEthApi.query.program.getIds();
      if (ids.includes(vftId)) {
        appeared = true;
      }
    }

    expect(appeared).toBeTruthy();
  });

  test("should approve wvara", async () => {
    const tx = await ethereumClient.wvara.approve(vftId, TOP_UP_AMOUNT);
    const receipt = await tx.sendAndWaitForReceipt();

    expect(receipt.status).toBe("success");
  });

  test("should top up executable balance", async () => {
      let newStateHash: Hex | undefined = undefined;
  
      mirror = getMirrorClient(vftId, walletClient, publicClient);
  
      const unwatch = mirror.watchStateChangedEvent((hash) => {
        newStateHash = hash;
      });
  
      const tx = await mirror.executableBalanceTopUp(TOP_UP_AMOUNT);
  
      const receipt = await tx.sendAndWaitForReceipt();
  
      expect(receipt.status).toBe("success");
  
      while (!newStateHash) {
        await wait1Block();
      }
  
      unwatch();
  
      stateHash = newStateHash;
    });

   test("should check executable balance", async () => {
      const state = await varaEthApi.query.program.readState(stateHash);
  
      expect(BigInt(state.executableBalance)).toBe(TOP_UP_AMOUNT);
    });
  
    test("should parse idl", () => {
        sails.parseIdl(idlContent);
      });

    test("should send init messages", async () => {
        const tx = await mirror.sendMessage(sails.ctors.New.encodePayload("Name", "Symbol", "12"));
        const result = await tx.sendAndWaitForReceipt();
        expect(result.status).toBe("success");
        const { waitForReply } = await tx.setupReplyListener();
    
        const reply = await waitForReply();
    
        expect(reply.replyCode).toBe("0x00000000");
        await wait1Block();
      });
});


describe("metadata", () => {
    test("reads name", async () => {
      const queryPayload = sails.services.Vft.queries.Name.encodePayload();
      const queryReply =
        await varaEthApi.call.program.calculateReplyForHandle(
          ethereumClient.accountAddress,
          vftId,
          queryPayload as `0x${string}`,
        );

      const name = sails.services.Vft.queries.Name.decodeResult(
        queryReply.payload,
      );
      expect(name).toBe("Name");
    });

    test("reads symbol", async () => {
      const queryPayload = sails.services.Vft.queries.Symbol.encodePayload();
      const queryReply =
        await varaEthApi.call.program.calculateReplyForHandle(
          ethereumClient.accountAddress,
          vftId,
          queryPayload as `0x${string}`,
        );

      const symbol = sails.services.Vft.queries.Symbol.decodeResult(
        queryReply.payload,
      );
      expect(symbol).toBe("Symbol");
    });

    test("reads decimals", async () => {
      const queryPayload = sails.services.Vft.queries.Decimals.encodePayload();
      const queryReply =
        await varaEthApi.call.program.calculateReplyForHandle(
          ethereumClient.accountAddress,
          vftId,
          queryPayload as `0x${string}`,
        );

      const decimals = sails.services.Vft.queries.Decimals.decodeResult(
        queryReply.payload,
      );
      expect(decimals).toBe(12);
    });
});

describe("send messages: mint", () => {
      let mirror: MirrorClient;
      const varaAddress = "kGfXzQ99jakxFMQEoxAEy4kSdK4hat21vVWF1PtL1BWaxeyts";
      const varaAmount = "10000000000000"
      test("should mint tokens", async () => {
        mirror = getMirrorClient(vftId, walletClient, publicClient);
        const payload = sails.services.Vft.functions.Mint.encodePayload(varaAddress, varaAmount);

        const tx = await mirror.sendMessage(payload);
        const receipt = await tx.sendAndWaitForReceipt();

        expect(receipt.status).toBe("success");

        const { waitForReply } = await tx.setupReplyListener();
        const reply = await waitForReply();

        expect(reply.replyCode).toBe("0x00010000");
        const result = sails.services.Vft.functions.Mint.decodeResult(reply.payload);
        console.log('Result:', result);
        expect(result).toBe(true);
        await wait1Block();
     });

     test("should return the increased balance", async () => {
        const queryPayload = sails.services.Vft.queries.BalanceOf.encodePayload(varaAddress);

        const queryReply = await varaEthApi.call.program.calculateReplyForHandle(
            ethereumClient.accountAddress,
            vftId,
            queryPayload as `0x${string}`,
        );

        const balance = sails.services.Vft.queries.BalanceOf.decodeResult(
            queryReply.payload,
        );
         expect(balance).toBe(varaAmount);
      
     });
        
     test("program executable balance decreases after calls", async () => {
        let state = await varaEthApi.query.program.readState(stateHash);
  
        const balanceBefore = state.executableBalance;
        const newStateHash = await mirror.stateHash();
        state = await varaEthApi.query.program.readState(newStateHash);
        const balanceAfter = state.executableBalance;
        expect(balanceAfter).toBeLessThan(balanceBefore);

     });
 }); 

 describe("injected txs: transfer", () => {
      const varaAddress = "kGjUf8Xv29hYnBcmP4MH6w3nkgHYDecNrEp55ho5db9iGrEyS";
      const varaAmount = "10000000000000"
      test("should transfer tokens", async () => {
        const payload = sails.services.Vft.functions.Transfer.encodePayload(varaAddress, varaAmount);
        const injected = await varaEthApi.createInjectedTransaction({
                destination: vftId,              
                payload, // Encoded message payload
                value: 0n,    
                recipient: '0xCC4E78EA999374E348E6D583af19b0F0E6689DE8'                      
              });
        // const result = await injected.send();;
        // expect(result).toBe("Accept");
        const promise = await injected.sendAndWaitForPromise();
        console.log(promise)
        console.log(promise.reply.code)
     });

     test("should return the increased balance", async () => {

        const queryPayload = sails.services.Vft.queries.BalanceOf.encodePayload(varaAddress);

        const queryReply = await varaEthApi.call.program.calculateReplyForHandle(
            ethereumClient.accountAddress,
            vftId,
            queryPayload as `0x${string}`,
        );

        const balance = sails.services.Vft.queries.BalanceOf.decodeResult(
            queryReply.payload,
        );
        expect(balance).toBe(varaAmount);

     });
});


 describe("injected txs: mint", () => {
      const varaAddress = "kGjUf8Xv29hYnBcmP4MH6w3nkgHYDecNrEp55ho5db9iGrEyS";
      const varaAmount = "10000000000000"
      test("should mint tokens", async () => {
        const payload = sails.services.Vft.functions.Mint.encodePayload(varaAddress, varaAmount);
        const injected = await varaEthApi.createInjectedTransaction( {
              destination: vftId,
              payload,
              value: 0n,
              recipient: '0xaEe0Cc6CAa1cFbee638470a995b9Bb75c1aB0972'
        });

        // const result = await injected.send();
        // expect(result).toBe("Accept");
        const promise = await injected.sendAndWaitForPromise();
        console.log(promise)
        console.log(promise.reply.code)
        
     });

     test("should return the increased balance", async () => {

        const queryPayload = sails.services.Vft.queries.BalanceOf.encodePayload(varaAddress);

        const queryReply = await varaEthApi.call.program.calculateReplyForHandle(
            ethereumClient.accountAddress,
            vftId,
            queryPayload as `0x${string}`,
        );

        const balance = sails.services.Vft.queries.BalanceOf.decodeResult(
            queryReply.payload,
        );
        expect(balance).toBe(varaAmount);

     });
});