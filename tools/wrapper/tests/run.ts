// どこで: wrapperテスト / 何を: 主要ロジックのユニットテストを実行 / なぜ: request_id導出・状態統合・execution参照の退行を防ぐため

import assert from "node:assert/strict";
import { AnonymousIdentity } from "@dfinity/agent";
import { mergeStatus } from "../lib/merge";
import {
  decimalToBytes32,
  deriveWrapRequestId,
  encodeUnwrapPayload,
  WRAP_PRECOMPILE_ADDRESS,
} from "../lib/request-id";
import { principalTextToBytes } from "../lib/principal";
import { bytesToHex, hexToBytes, parseRequestIdHex } from "../lib/utils";
import {
  estimateUnwrapGasLimit,
  estimateWrapGasLimit,
  getUnwrapRequestIdsByTxId,
  getWrapEvmNonce,
  submitIcTx,
  wrapperClientTestHooks,
} from "../lib/canister/wrapper-client";
import { getExecutionResult } from "../lib/canister/wrap-client";
import { resolveUnwrapBurnSpenderEvmAddress } from "../lib/canister/erc20-client";
import {
  messageAfterRefreshSuccess,
  nextPollFailureState,
  shouldScheduleAutoPolling,
} from "../lib/status-poll";
import {
  computeRequiredAllowances,
  computeWrapFeeQuote,
  deriveStatusPhase,
  isTerminalStatus,
} from "../lib/wrap-flow";
import { parsePositiveU64 } from "../lib/wrap-input";
import {
  applyWrapGasHeadroom,
  applyUnwrapGasHeadroom,
  buildUnwrapEstimateCallObject,
  buildWrapEstimateCallObject,
  encodeFactoryMintForAssetCallData,
  validateEstimatedGasLimit,
} from "../lib/wrap-estimate";
import {
  dedupeAssetOptions,
  mergeAssetOptions,
  normalizeCustomAssetDraft,
  parseStoredCustomAssets,
  serializeCustomAssets,
} from "../lib/asset-catalog";
import { configTestHooks, loadConfig } from "../lib/config";
import { iiTestHooks } from "../lib/wallet/ii";
import {
  decodeAddressReturnData,
  decodeUint256ReturnData,
  encodeAllowanceCall,
  encodeApproveCall,
  encodeFactoryGetTokenAddressCall,
} from "../lib/erc20";
import { icrcClientTestHooks } from "../lib/canister/icrc2-client";

async function runUtilsTests(): Promise<void> {
  const value = Uint8Array.from([0x01, 0xab, 0x10]);
  const hex = bytesToHex(value);
  assert.equal(hex, "0x01ab10");

  const decoded = hexToBytes(hex);
  assert.deepEqual(decoded, value);

  assert.throws(() => hexToBytes("0x0"));
  assert.throws(() => parseRequestIdHex("0x11"));
  assert.doesNotThrow(() => parseRequestIdHex(`0x${"11".repeat(32)}`));
}

async function runRequestIdTests(): Promise<void> {
  assert.equal(
    Buffer.from(WRAP_PRECOMPILE_ADDRESS).toString("hex"),
    "00000000000000000000000000000000ffff0001",
  );

  const payload = encodeUnwrapPayload({
    assetId: "2vxsx-fae",
    amount: 1n,
    recipient: "2vxsx-fae",
  });
  assert.equal(payload.length, 93);
  assert.equal(payload[0], 1);

  const wrapRequestId = deriveWrapRequestId({
    // 匿名principalではなく、実運用で使う非匿名principal bytesを使う。
    fromOwner: principalTextToBytes("4c52m-aiaaa-aaaam-agwwa-cai"),
    assetId: principalTextToBytes("2vxsx-fae"),
    amount: decimalToBytes32("1000000000000000000"),
    evmRecipient: hexToBytes("0x1111111111111111111111111111111111111111"),
    evmNonce: 1n,
    gasLimit: 300_000n,
  });
  assert.equal(wrapRequestId.length, 32);
}

async function runMergeTests(): Promise<void> {
  const merged = mergeStatus({
    requestIdHex: `0x${"11".repeat(32)}`,
    dispatchResult: {
      status: "Dispatched",
      errorCode: null,
    },
    executionResult: {
      status: "Succeeded",
      ledgerTxId: Uint8Array.from([9, 8]),
      errorCode: null,
      mintFailedRecoverable: false,
      withdrawn: false,
      withdrawLedgerTxId: null,
      withdrawErrorCode: null,
    },
  });
  assert.equal(merged.dispatchStatus, "Dispatched");
  assert.equal(merged.executionStatus, "Succeeded");
  assert.equal(merged.ledgerTxId, "0x0908");
  assert.equal(merged.withdrawn, false);
}

async function runExecutionBranchTests(): Promise<void> {
  const requestId = Uint8Array.from(new Array(32).fill(0x11));

  const unwrapOnly = await getExecutionResult(requestId, {
    readUnwrapResult: async () => [{
      status: { Succeeded: null },
      ledger_tx_id: [Uint8Array.from([0xaa, 0xbb])],
      error_code: [],
    }],
    readWrapResult: async () => [],
  });
  assert.equal(unwrapOnly?.status, "Succeeded");
  assert.deepEqual(unwrapOnly?.ledgerTxId, Uint8Array.from([0xaa, 0xbb]));

  const wrapPreferred = await getExecutionResult(requestId, {
    readUnwrapResult: async () => [{
      status: { Failed: null },
      ledger_tx_id: [],
      error_code: ["unwrap_failed"],
    }],
    readWrapResult: async () => [{
      status: { Failed: null },
      pull_ledger_tx_id: [Uint8Array.from([0x01])],
      mint_tx_id: [],
      error_code: ["wrap_failed"],
      withdrawn: false,
      withdraw_ledger_tx_id: [],
      withdraw_error_code: [],
      mint_failed_recoverable: true,
      fee_ledger_tx_id: [],
      charged_fee_e8s: [],
      charged_gas_price_wei: [],
    }],
  });
  assert.equal(wrapPreferred?.errorCode, "wrap_failed");
  assert.equal(wrapPreferred?.mintFailedRecoverable, true);
}

async function runFeeQuoteMathTests(): Promise<void> {
  const quote = computeWrapFeeQuote({
    gasPriceWei: 250_000_000_000n,
    gasLimit: 300_000n,
    cycleFeeE8s: 1_000_000n,
    gasPriceBufferBps: 12_000n,
  });
  assert.equal(quote.chargedGasPriceWei, 300_000_000_000n);
  assert.equal(quote.totalFeeE8s, 10_000_000n);
}

async function runAllowanceTests(): Promise<void> {
  const separate = computeRequiredAllowances({
    assetLedgerCanister: "a",
    feeLedgerCanister: "b",
    amount: 200n,
    totalFeeE8s: 50n,
  });
  assert.equal(separate.requiredAssetAllowance, 200n);
  assert.equal(separate.requiredFeeAllowance, 50n);

  const merged = computeRequiredAllowances({
    assetLedgerCanister: "a",
    feeLedgerCanister: "a",
    amount: 200n,
    totalFeeE8s: 50n,
  });
  assert.equal(merged.requiredAssetAllowance, 250n);
  assert.equal(merged.requiredFeeAllowance, 0n);
}

async function runWrapInputValidationTests(): Promise<void> {
  assert.equal(parsePositiveU64("1", "validation.gas_limit.invalid"), 1n);
  assert.throws(
    () => parsePositiveU64("0", "validation.gas_limit.invalid"),
    /validation\.gas_limit\.invalid/,
  );
}

async function runWrapEstimateEncodingTests(): Promise<void> {
  const data = encodeFactoryMintForAssetCallData({
    assetId: principalTextToBytes("2vxsx-fae"),
    tokenDecimals: 8,
    evmRecipient: hexToBytes("0x1111111111111111111111111111111111111111"),
    amount: decimalToBytes32("1000000000000000000"),
  });
  assert.equal(data.length % 32, 4);

  const call = buildWrapEstimateCallObject({
    wrapCanisterId: "4c52m-aiaaa-aaaam-agwwa-cai",
    evmWrapFactory: "0x2222222222222222222222222222222222222222",
    assetId: "2vxsx-fae",
    tokenDecimals: 8,
    amount: "1000000000000000000",
    evmRecipient: "0x1111111111111111111111111111111111111111",
  });
  assert.equal(call.to.length, 1);
  assert.equal(call.from.length, 1);
  assert.equal(call.value.length, 1);
  assert.equal(call.data.length, 1);
  assert.equal(call.data[0]?.length, data.length);
  assert.equal(
    Buffer.from(call.data[0]?.subarray(0, 4) ?? new Uint8Array()).toString("hex"),
    Buffer.from(data.subarray(0, 4)).toString("hex"),
  );
  const unwrapCall = buildUnwrapEstimateCallObject({
    callerEvmAddress: hexToBytes("0x1111111111111111111111111111111111111111"),
    nonce: 7n,
    data: Uint8Array.from([0xaa, 0xbb]),
  });
  assert.equal(unwrapCall.to.length, 1);
  assert.equal(unwrapCall.from.length, 1);
  assert.equal(unwrapCall.nonce.length, 1);
  assert.equal(unwrapCall.data.length, 1);
  assert.equal(applyWrapGasHeadroom(43_574n), 52_289n);
  assert.equal(applyUnwrapGasHeadroom(250_000n), 300_000n);
  assert.equal(applyUnwrapGasHeadroom(300_000n), 360_000n);
}

async function runErc20EncodingTests(): Promise<void> {
  const tokenCall = encodeFactoryGetTokenAddressCall("2vxsx-fae");
  assert.equal(tokenCall.length % 32, 4);
  assert.deepEqual(
    resolveUnwrapBurnSpenderEvmAddress("0x5555555555555555555555555555555555555555"),
    hexToBytes("0x5555555555555555555555555555555555555555"),
  );

  const allowanceCall = encodeAllowanceCall(
    hexToBytes("0x1111111111111111111111111111111111111111"),
    hexToBytes("0x2222222222222222222222222222222222222222"),
  );
  assert.equal(allowanceCall.length, 68);

  const approveCall = encodeApproveCall(
    hexToBytes("0x3333333333333333333333333333333333333333"),
    7n,
  );
  assert.equal(approveCall.length, 68);

  const addressWord = new Uint8Array(32);
  addressWord.set(hexToBytes("0x4444444444444444444444444444444444444444"), 12);
  assert.deepEqual(
    decodeAddressReturnData(addressWord),
    hexToBytes("0x4444444444444444444444444444444444444444"),
  );

  const amountWord = new Uint8Array(32);
  amountWord[31] = 9;
  assert.equal(decodeUint256ReturnData(amountWord), 9n);
}

async function runLedgerMetadataTests(): Promise<void> {
  assert.equal(
    icrcClientTestHooks.decodeLedgerDecimals([
      ["icrc1:name", { Text: "Token" }],
      ["icrc1:decimals", { Nat: 8n }],
    ]),
    8,
  );
  assert.throws(
    () => icrcClientTestHooks.decodeLedgerDecimals([["icrc1:decimals", { Text: "8" }]]),
    /wrap\.asset_decimals_invalid/,
  );
  assert.throws(
    () => icrcClientTestHooks.decodeLedgerDecimals([["icrc1:name", { Text: "Token" }]]),
    /wrap\.asset_metadata_failed:decimals_missing/,
  );
}

async function runEstimateWrapGasClientTests(): Promise<void> {
  const gas = await estimateWrapGasLimit(
    {
      wrapCanisterId: "4c52m-aiaaa-aaaam-agwwa-cai",
      evmWrapFactory: "0x2222222222222222222222222222222222222222",
      assetId: "2vxsx-fae",
      tokenDecimals: 8,
      amount: "1000000000000000000",
      evmRecipient: "0x1111111111111111111111111111111111111111",
    },
    {
      readEstimateGas: async () => ({ Ok: 300_000n }),
    },
  );
  assert.equal(gas, 360_000n);
  assert.equal(validateEstimatedGasLimit(21_000n), 21_000n);
  assert.throws(() => validateEstimatedGasLimit(0n), /wrap\.estimate_gas_invalid/);

  await assert.rejects(
    () => estimateWrapGasLimit(
      {
        wrapCanisterId: "4c52m-aiaaa-aaaam-agwwa-cai",
        evmWrapFactory: "0x2222222222222222222222222222222222222222",
        assetId: "2vxsx-fae",
        tokenDecimals: 8,
        amount: "1000000000000000000",
        evmRecipient: "0x1111111111111111111111111111111111111111",
      },
      {
        readEstimateGas: async () => ({ Err: { code: 32000, message: "revert", error_prefix: [] } }),
      },
    ),
    /evm_gateway\.estimate_gas_failed:32000:revert/,
  );
  assert.equal(
    wrapperClientTestHooks.decodeRpcNatError("evm_gateway.estimate_gas_failed", {
      code: 32000,
      message: "revert",
      error_prefix: [],
    }),
    "evm_gateway.estimate_gas_failed:32000:revert",
  );
}

async function runEstimateUnwrapGasClientTests(): Promise<void> {
  const gas = await estimateUnwrapGasLimit(
    {
      callerEvmAddress: hexToBytes("0x1111111111111111111111111111111111111111"),
      nonce: 9n,
      data: Uint8Array.from([0xaa]),
    },
    {
      readEstimateGas: async () => ({ Ok: 400_000n }),
    },
  );
  assert.equal(gas, 480_000n);

  await assert.rejects(
    () => estimateUnwrapGasLimit(
      {
        callerEvmAddress: hexToBytes("0x1111111111111111111111111111111111111111"),
        nonce: 9n,
        data: Uint8Array.from([0xaa]),
      },
      {
        readEstimateGas: async () => ({ Err: { code: 32000, message: "revert", error_prefix: [] } }),
      },
    ),
    /evm_gateway\.estimate_gas_failed:32000:revert/,
  );
}

async function runWrapNonceClientTests(): Promise<void> {
  let capturedLength: number | null = null;
  const nonce = await getWrapEvmNonce("4c52m-aiaaa-aaaam-agwwa-cai", {
    readExpectedNonce: async (address: Uint8Array) => {
      capturedLength = address.length;
      return 7n;
    },
  });
  assert.equal(nonce, 7n);
  if (capturedLength === null) {
    throw new Error("captured nonce address missing");
  }
  assert.equal(capturedLength, 20);
}

async function runAssetCatalogTests(): Promise<void> {
  const custom = normalizeCustomAssetDraft({
    label: "My Token",
    assetId: "2vxsx-fae",
  });
  assert.equal(custom.source, "custom");

  assert.throws(
    () => normalizeCustomAssetDraft({ label: "", assetId: "2vxsx-fae" }),
    /validation\.asset_label_required/,
  );
  assert.throws(
    () => normalizeCustomAssetDraft({ label: "Bad", assetId: "not-a-principal" }),
  );

  const merged = mergeAssetOptions([
    custom,
    { assetId: "2vxsx-fae", label: "Duplicate", source: "custom" },
  ]);
  assert.ok(merged.length >= 5);
  assert.equal(merged.filter((asset) => asset.assetId === "2vxsx-fae").length, 1);

  const serialized = serializeCustomAssets([custom]);
  const parsed = parseStoredCustomAssets(serialized);
  assert.deepEqual(parsed, [custom]);
  assert.deepEqual(dedupeAssetOptions([custom, custom]), [custom]);
}

async function runInternetIdentityConfigTests(): Promise<void> {
  const testEnvBase: NodeJS.ProcessEnv = {
    NODE_ENV: "test",
  };
  assert.throws(
    () => loadConfig(testEnvBase),
    /config\.missing:NEXT_PUBLIC_IC_HOST/,
  );
  assert.throws(
    () => loadConfig({
      ...testEnvBase,
      NEXT_PUBLIC_IC_HOST: "http://127.0.0.1:8000",
    }),
    /config\.missing:KASANE_EVM_CANISTER_ID/,
  );
  assert.throws(
    () => loadConfig({
      ...testEnvBase,
      NEXT_PUBLIC_IC_HOST: "http://127.0.0.1:8000",
      KASANE_EVM_CANISTER_ID: "4c52m-aiaaa-aaaam-agwwa-cai",
    }),
    /config\.missing:WRAP_CANISTER_ID/,
  );
  assert.throws(
    () => loadConfig({
      ...testEnvBase,
      NEXT_PUBLIC_IC_HOST: "http://127.0.0.1:8000",
      KASANE_EVM_CANISTER_ID: "4c52m-aiaaa-aaaam-agwwa-cai",
      WRAP_CANISTER_ID: "t63gs-up777-77776-aaaba-cai",
    }),
    /config\.missing:EVM_WRAP_FACTORY/,
  );
  assert.deepEqual(
    loadConfig({
      ...testEnvBase,
      NEXT_PUBLIC_IC_HOST: "http://127.0.0.1:8000",
      KASANE_EVM_CANISTER_ID: "4c52m-aiaaa-aaaam-agwwa-cai",
      WRAP_CANISTER_ID: "t63gs-up777-77776-aaaba-cai",
      EVM_WRAP_FACTORY: "0x88200f183e26d05bc6747ba7378cc73a68b6a12a",
    }),
    {
      icHost: "http://127.0.0.1:8000",
      kasaneEvmCanisterId: "4c52m-aiaaa-aaaam-agwwa-cai",
      wrapCanisterId: "t63gs-up777-77776-aaaba-cai",
      evmWrapFactory: "0x88200f183e26d05bc6747ba7378cc73a68b6a12a",
    },
  );
  assert.equal(
    iiTestHooks.resolveIdentityProvider(null),
    "https://identity.ic0.app",
  );
  assert.equal(
    iiTestHooks.resolveIdentityProvider("http://rdmx6-jaaaa-aaaaa-aaadq-cai.localhost:8000"),
    "http://rdmx6-jaaaa-aaaaa-aaadq-cai.localhost:8000",
  );
  assert.equal(
    configTestHooks.resolveConfiguredIdentityProvider({
      ...process.env,
      NEXT_PUBLIC_INTERNET_IDENTITY_URL: "",
    }),
    null,
  );
  assert.equal(
    configTestHooks.resolveConfiguredIdentityProvider({
      ...process.env,
      NEXT_PUBLIC_INTERNET_IDENTITY_URL: "http://rdmx6-jaaaa-aaaaa-aaadq-cai.localhost:8000",
    }),
    "http://rdmx6-jaaaa-aaaaa-aaadq-cai.localhost:8000",
  );
  assert.equal(configTestHooks.shouldFetchRootKey("http://127.0.0.1:8000"), true);
  assert.equal(configTestHooks.shouldFetchRootKey("http://localhost:8000"), true);
  assert.equal(configTestHooks.shouldFetchRootKey("https://icp-api.io"), false);
}

async function runStatusPhaseTests(): Promise<void> {
  assert.equal(deriveStatusPhase(null), "idle");
  assert.equal(
    deriveStatusPhase({
      requestId: "0x11",
      dispatchStatus: "Queued",
      executionStatus: null,
      ledgerTxId: null,
      errorCode: null,
      mintFailedRecoverable: false,
      withdrawn: false,
      withdrawLedgerTxId: null,
      withdrawErrorCode: null,
    }),
    "submitted",
  );
  assert.equal(
    deriveStatusPhase({
      requestId: "0x11",
      dispatchStatus: "Dispatched",
      executionStatus: "Running",
      ledgerTxId: null,
      errorCode: null,
      mintFailedRecoverable: false,
      withdrawn: false,
      withdrawLedgerTxId: null,
      withdrawErrorCode: null,
    }),
    "executing",
  );
  assert.equal(
    isTerminalStatus({
      requestId: "0x11",
      dispatchStatus: "Dispatched",
      executionStatus: "Succeeded",
      ledgerTxId: null,
      errorCode: null,
      mintFailedRecoverable: false,
      withdrawn: false,
      withdrawLedgerTxId: null,
      withdrawErrorCode: null,
    }),
    true,
  );
  assert.equal(
    isTerminalStatus({
      requestId: "0x11",
      dispatchStatus: "Dispatching",
      executionStatus: "Running",
      ledgerTxId: null,
      errorCode: null,
      mintFailedRecoverable: false,
      withdrawn: false,
      withdrawLedgerTxId: null,
      withdrawErrorCode: null,
    }),
    false,
  );
}

async function runStatusPollingRegressionTests(): Promise<void> {
  const nonTerminalStatus = {
    requestId: "0x11",
    dispatchStatus: "Dispatching" as const,
    executionStatus: "Running" as const,
    ledgerTxId: null,
    errorCode: null,
    mintFailedRecoverable: false,
    withdrawn: false,
    withdrawLedgerTxId: null,
    withdrawErrorCode: null,
  };
  assert.equal(
    shouldScheduleAutoPolling({
      autoPolling: true,
      status: nonTerminalStatus,
      pollFailureCount: 0,
    }),
    true,
  );
  assert.equal(
    shouldScheduleAutoPolling({
      autoPolling: true,
      status: nonTerminalStatus,
      pollFailureCount: 3,
    }),
    false,
  );
  assert.equal(
    shouldScheduleAutoPolling({
      autoPolling: true,
      status: {
        ...nonTerminalStatus,
        executionStatus: "Succeeded",
      },
      pollFailureCount: 0,
    }),
    false,
  );

  const firstFailure = nextPollFailureState({ currentFailureCount: 0 });
  assert.equal(firstFailure.nextFailureCount, 1);
  assert.equal(firstFailure.shouldStop, false);

  const thirdFailure = nextPollFailureState({ currentFailureCount: 2 });
  assert.equal(thirdFailure.nextFailureCount, 3);
  assert.equal(thirdFailure.shouldStop, true);

  assert.equal(
    messageAfterRefreshSuccess({
      currentMessage: "status.auto_poll_stopped",
      background: true,
    }),
    null,
  );
  assert.equal(
    messageAfterRefreshSuccess({
      currentMessage: "wallet.not_connected",
      background: true,
    }),
    "wallet.not_connected",
  );
  assert.equal(
    messageAfterRefreshSuccess({
      currentMessage: "status.auto_poll_stopped",
      background: false,
    }),
    null,
  );
}

function buildMockQueryActor(args: {
  gasPriceResult: { Ok: bigint } | { Err: { code: number; message: string; error_prefix: [] | [string] } };
  priorityFeeResult: { Ok: bigint } | { Err: { code: number; message: string; error_prefix: [] | [string] } };
}) {
  const noRevertData: [] = [];
  return {
    expected_nonce_by_address: async () => ({ Ok: 0n }),
    rpc_eth_gas_price: async () => args.gasPriceResult,
    rpc_eth_max_priority_fee_per_gas: async () => args.priorityFeeResult,
    rpc_eth_estimate_gas_object: async () => ({ Ok: 300_000n }),
    rpc_eth_call_object: async () => ({ Ok: {
      status: 1,
      gas_used: 21_000n,
      return_data: new Uint8Array(32),
      revert_data: noRevertData,
    } }),
    get_request_dispatch_result: async (): Promise<[]> => [],
    get_unwrap_request_ids_by_tx_id: async () => [],
  };
}

async function runGetUnwrapRequestIdsByTxIdTests(): Promise<void> {
  wrapperClientTestHooks.reset();
  wrapperClientTestHooks.setMockQueryActor({
    ...buildMockQueryActor({
      gasPriceResult: { Ok: 1n },
      priorityFeeResult: { Ok: 1n },
    }),
    get_unwrap_request_ids_by_tx_id: async (txId) => [
      Uint8Array.from(txId),
      Uint8Array.from(new Array(32).fill(0x22)),
    ],
  });

  const out = await getUnwrapRequestIdsByTxId(Uint8Array.from(new Array(32).fill(0x11)));
  assert.equal(out.length, 2);
  assert.deepEqual(out[1], Uint8Array.from(new Array(32).fill(0x22)));
}

async function runWrapperClientFeeTests(): Promise<void> {
  wrapperClientTestHooks.reset();
  const submittedArgsList: Array<{
    to: [] | [Uint8Array];
    value: bigint;
    max_priority_fee_per_gas: bigint;
    data: Uint8Array;
    max_fee_per_gas: bigint;
    nonce: bigint;
    gas_limit: bigint;
  }> = [];

  wrapperClientTestHooks.setMockQueryActor(buildMockQueryActor({
    gasPriceResult: { Ok: 250_000_000_000n },
    priorityFeeResult: { Ok: 2_000_000_000n },
  }));
  wrapperClientTestHooks.setMockSubmitActor({
    submit_ic_tx: async (args) => {
      submittedArgsList.push(args);
      return { Ok: Uint8Array.from([0xaa]) };
    },
  });

  const txId = await submitIcTx({
    to: Uint8Array.from(new Array(20).fill(0x11)),
    data: Uint8Array.from([0x01, 0x02]),
    nonce: 7n,
    gasLimit: 333_000n,
    identity: new AnonymousIdentity(),
  });
  assert.deepEqual(txId, Uint8Array.from([0xaa]));
  const submittedArgs = submittedArgsList[0];
  if (submittedArgs === undefined) {
    throw new Error("submit args missing");
  }
  assert.equal(submittedArgs.max_fee_per_gas, 250_000_000_000n);
  assert.equal(submittedArgs.max_priority_fee_per_gas, 2_000_000_000n);
  assert.equal(submittedArgs.gas_limit, 333_000n);

  wrapperClientTestHooks.setMockQueryActor(buildMockQueryActor({
    gasPriceResult: { Err: { code: 32000, message: "state unavailable", error_prefix: [] } },
    priorityFeeResult: { Ok: 2_000_000_000n },
  }));
  await assert.rejects(
    submitIcTx({
      to: Uint8Array.from(new Array(20).fill(0x11)),
      data: Uint8Array.from([0x01]),
      nonce: 8n,
      gasLimit: 300_000n,
      identity: new AnonymousIdentity(),
    }),
    /evm_gateway\.gas_price_failed:32000:state unavailable/,
  );

  wrapperClientTestHooks.setMockQueryActor(buildMockQueryActor({
    gasPriceResult: { Ok: 250_000_000_000n },
    priorityFeeResult: { Err: { code: 32000, message: "state unavailable", error_prefix: [] } },
  }));
  await assert.rejects(
    submitIcTx({
      to: Uint8Array.from(new Array(20).fill(0x11)),
      data: Uint8Array.from([0x01]),
      nonce: 9n,
      gasLimit: 300_000n,
      identity: new AnonymousIdentity(),
    }),
    /evm_gateway\.priority_fee_failed:32000:state unavailable/,
  );
  wrapperClientTestHooks.reset();
}

async function main(): Promise<void> {
  await runUtilsTests();
  await runRequestIdTests();
  await runMergeTests();
  await runExecutionBranchTests();
  await runFeeQuoteMathTests();
  await runAllowanceTests();
  await runWrapInputValidationTests();
  await runWrapEstimateEncodingTests();
  await runErc20EncodingTests();
  await runLedgerMetadataTests();
  await runEstimateWrapGasClientTests();
  await runEstimateUnwrapGasClientTests();
  await runWrapNonceClientTests();
  await runAssetCatalogTests();
  await runInternetIdentityConfigTests();
  await runStatusPhaseTests();
  await runStatusPollingRegressionTests();
  await runGetUnwrapRequestIdsByTxIdTests();
  await runWrapperClientFeeTests();
  process.stdout.write("wrapper tests passed\n");
}

main().catch((error: unknown) => {
  process.stderr.write(`${error instanceof Error ? error.stack ?? error.message : String(error)}\n`);
  process.exitCode = 1;
});
