import { bytesToHex, hexToBytes, Hex } from "viem";
import { ReplyCode } from "@vara-eth/api";
import { expect } from "vitest";

export const AUTO_SUCCESS_REPLY_CODE = "0x00000000";
export const MANUAL_SUCCESS_REPLY_CODE = "0x00010000";

type ReplyLike = {
  payload: Hex | string;
  value: bigint;
};

type MirrorReplyLike = ReplyLike & {
  replyCode: string;
};

type InjectedReplyLike = ReplyLike & {
  code: ReplyCode;
};

function normalizeCode(code: string | ReplyCode): Hex {
  return typeof code === "string" ? (code as Hex) : bytesToHex(code.toBytes());
}

export function isSuccessReplyCode(code: string | ReplyCode) {
  return typeof code === "string"
    ? ReplyCode.fromBytes(normalizeCode(code)).isSuccess
    : code.isSuccess;
}

export function isErrorReplyCode(code: string | ReplyCode) {
  return typeof code === "string"
    ? ReplyCode.fromBytes(normalizeCode(code)).isError
    : code.isError;
}

export function expectAutoSuccessReply(reply: MirrorReplyLike) {
  expect(reply.replyCode).toBe(AUTO_SUCCESS_REPLY_CODE);
  expect(reply.payload).toBe("0x");
  expect(reply.value).toBe(0n);
}

export function expectManualSuccessReply(
  reply: MirrorReplyLike,
  expectedPayload?: Hex | string,
) {
  expect(reply.replyCode).toBe(MANUAL_SUCCESS_REPLY_CODE);
  if (expectedPayload !== undefined) {
    expect(reply.payload).toBe(expectedPayload);
  }
}

export function expectAutoSuccessPromise(result: InjectedReplyLike) {
  expect(normalizeCode(result.code)).toBe(AUTO_SUCCESS_REPLY_CODE);
  expect(result.payload).toBe("0x");
  expect(result.value).toBe(0n);
}

export function expectManualSuccessPromise(
  result: InjectedReplyLike,
  expectedPayload?: Hex | string,
) {
  expect(normalizeCode(result.code)).toBe(MANUAL_SUCCESS_REPLY_CODE);
  if (expectedPayload !== undefined) {
    expect(result.payload).toBe(expectedPayload);
  }
}

export function expectErrorReplyCode(code: string | ReplyCode) {
  expect(isErrorReplyCode(code)).toBe(true);
}
