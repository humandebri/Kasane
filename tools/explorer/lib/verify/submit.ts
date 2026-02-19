// どこで: verify submit補助層 / 何を: 重複競合を含む投入処理を共通化 / なぜ: API本体を薄くしテスト容易性を上げるため

import { getVerifyRequestBySubmittedUserAndInputHash, insertVerifyRequest } from "../db";

export type VerifyRequestStatus = "queued" | "running" | "succeeded" | "failed";

type VerifyRequestLookup = {
  id: string;
  status: VerifyRequestStatus;
};

export type CreateOrGetVerifyRequestInput = {
  inputHash: string;
  id: string;
  contractAddress: string;
  chainId: number;
  submittedBy: string;
  payloadCompressed: Uint8Array;
  createdAtMs: bigint;
};

export type CreateOrGetVerifyRequestDeps = {
  getByInputHash: (submittedBy: string, inputHash: string) => Promise<VerifyRequestLookup | null>;
  insert: (input: {
    id: string;
    contractAddress: string;
    chainId: number;
    submittedBy: string;
    status: "queued";
    inputHash: string;
    payloadCompressed: Uint8Array;
    createdAtMs: bigint;
  }) => Promise<void>;
};

export type CreateOrGetVerifyRequestResult = {
  requestId: string;
  status: VerifyRequestStatus;
  created: boolean;
  httpStatus: 200 | 202;
};

const createOrGetVerifyRequestDefaultDeps: CreateOrGetVerifyRequestDeps = {
  getByInputHash: getVerifyRequestBySubmittedUserAndInputHash,
  insert: insertVerifyRequest,
};

export async function createOrGetVerifyRequest(
  input: CreateOrGetVerifyRequestInput,
  deps: CreateOrGetVerifyRequestDeps = createOrGetVerifyRequestDefaultDeps
): Promise<CreateOrGetVerifyRequestResult> {
  const existing = await deps.getByInputHash(input.submittedBy, input.inputHash);
  if (existing) {
    return { requestId: existing.id, status: existing.status, created: false, httpStatus: 200 };
  }
  try {
    await deps.insert({
      id: input.id,
      contractAddress: input.contractAddress,
      chainId: input.chainId,
      submittedBy: input.submittedBy,
      status: "queued",
      inputHash: input.inputHash,
      payloadCompressed: input.payloadCompressed,
      createdAtMs: input.createdAtMs,
    });
    return { requestId: input.id, status: "queued", created: true, httpStatus: 202 };
  } catch (err) {
    if (!isUniqueViolation(err)) {
      throw err;
    }
    const raced = await deps.getByInputHash(input.submittedBy, input.inputHash);
    if (!raced) {
      throw err;
    }
    return { requestId: raced.id, status: raced.status, created: false, httpStatus: 200 };
  }
}

export function isUniqueViolation(err: unknown): boolean {
  if (!isRecord(err)) {
    return false;
  }
  return err.code === "23505";
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
