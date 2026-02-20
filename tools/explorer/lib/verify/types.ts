// どこで: verify機能の型定義 / 何を: API入出力と内部結果を共通化 / なぜ: 画面・API・ワーカーで同一契約を保つため

export type VerifySourceBundle = Record<string, string>;

export type VerifySubmitInput = {
  chainId: number;
  contractAddress: string;
  compilerVersion: string;
  optimizerEnabled: boolean;
  optimizerRuns: number;
  evmVersion: string | null;
  sourceBundle: VerifySourceBundle;
  contractName: string;
  constructorArgsHex: string;
};

export type VerifyStatus = "queued" | "running" | "succeeded" | "failed";

export type VerifyRequestRow = {
  id: string;
  contractAddress: string;
  chainId: number;
  submittedBy: string;
  status: VerifyStatus;
  inputHash: string;
  payloadCompressed: Uint8Array;
  errorCode: string | null;
  errorMessage: string | null;
  startedAt: bigint | null;
  finishedAt: bigint | null;
  attempts: number;
  createdAt: bigint;
  updatedAt: bigint;
  verifiedContractId: string | null;
};

export type VerifiedContractRow = {
  id: string;
  contractAddress: string;
  chainId: number;
  contractName: string;
  compilerVersion: string;
  optimizerEnabled: boolean;
  optimizerRuns: number;
  evmVersion: string | null;
  creationMatch: boolean;
  runtimeMatch: boolean;
  abiJson: string;
  sourceBlobId: string;
  metadataBlobId: string;
  publishedAt: bigint;
};

export type VerifyJobErrorCode =
  | "invalid_input"
  | "compiler_unavailable"
  | "compile_error"
  | "compile_timeout"
  | "compile_resource_exceeded"
  | "compile_process_failed"
  | "contract_not_found"
  | "runtime_mismatch"
  | "deploy_tx_not_found"
  | "creation_mismatch"
  | "creation_input_missing"
  | "rpc_unavailable"
  | "sourcify_error"
  | "internal_error";

export type VerifyExecutionResult = {
  creationMatch: boolean;
  runtimeMatch: boolean;
  abiJson: string;
  metadataJson: string;
  sourcifyStatus: "full_match" | "partial_match" | "not_found" | "error";
};
