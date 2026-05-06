// どこで: wrapperテスト / 何を: 主要ロジックのユニットテストを実行 / なぜ: request_id導出・状態統合・execution参照の退行を防ぐため

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { AnonymousIdentity, type Identity } from "@icp-sdk/core/agent";
import { Principal } from "@icp-sdk/core/principal";
import { createActorCache } from "../lib/canister/actor-utils";
import { getIdentityAgent, getQueryAgent, resetAgentCache } from "../lib/canister/agent";
import { mergeStatus } from "../lib/merge";
import {
  decimalToBytes32,
  deriveWrapRequestId,
  encodeNativeWithdrawPayload,
  encodeUnwrapPayload,
  NATIVE_WITHDRAW_PRECOMPILE_ADDRESS,
  tokenAmountToBytes32,
  WRAP_PRECOMPILE_ADDRESS,
} from "../lib/request-id";
import { prepareNativeWithdrawTransaction } from "../lib/native-withdraw";
import { principalTextToBytes } from "../lib/principal";
import { bytesToHex, hexToBytes, parseRequestIdHex } from "../lib/utils";
import {
  estimateUnwrapGasLimit,
  estimateWrapGasLimit,
  getDispatchResult,
  getUnwrapRequestIdsByEthTxHash,
  getUnwrapRequestIdsByTxId,
  getWrapEvmNonce,
  submitIcTx,
  wrapperClientTestHooks,
} from "../lib/canister/wrapper-client";
import {
  getExecutionResult,
  quoteNativeWithdrawal,
  getUnwrapRequirements,
  submitWrapRequest,
  withdrawFailedWrap,
  wrapClientTestHooks,
} from "../lib/canister/wrap-client";
import {
  approveWrappedTokenIfNeeded,
  erc20ClientTestHooks,
  resolveUnwrapBurnSpenderEvmAddress,
} from "../lib/canister/erc20-client";
import {
  messageAfterRefreshSuccess,
  nextPollFailureState,
  shouldScheduleAutoPolling,
} from "../lib/status-poll";
import {
  computeRequiredAllowances,
  computeWrapFeeQuote,
  deriveStatusPhase,
  formatTokenAmount,
  formatTokenBalance2,
  formatE8sToIcpText4,
  formatWeiToGwei2,
  isTerminalStatus,
} from "../lib/wrap-flow";
import type { StatusResponse } from "../lib/types";
import { parsePositiveU64, parseTokenAmount } from "../lib/wrap-input";
import {
  applyWrapGasHeadroom,
  applyUnwrapGasHeadroom,
  buildUnwrapEstimateCallObject,
  buildWrapEstimateCallObject,
  encodeFactoryMintForAssetCallData,
  validateEstimatedGasLimit,
} from "../lib/wrap-estimate";
import {
  assetCatalogTestHooks,
  DEFAULT_ASSET_ID,
  dedupeAssetOptions,
  LOCAL_TEST_ASSET_ID,
  MAINNET_IC_HOST,
  mergeAssetOptions,
  normalizeCustomAssetDraft,
  parseStoredCustomAssets,
  presetAssetOptions,
  serializeCustomAssets,
} from "../lib/asset-catalog";
import { configTestHooks, loadConfig } from "../lib/config";
import { applySelectedAsset, normalizeIcpTokenList, toManageTokenOptions } from "../lib/icp-token-list";
import { recentRequestsClientTestHooks } from "../lib/canister/recent-requests-client";
import {
  createRecentRequestKey,
  mergeRecentRequestHistory,
  RecentRequestDocSchema,
} from "../lib/recent-requests";
import {
  decodeAddressReturnData,
  decodeUint256ReturnData,
  encodeAllowanceCall,
  encodeApproveCall,
  encodeFactoryGetTokenAddressCall,
} from "../lib/erc20";
import { icrcClientTestHooks } from "../lib/canister/icrc2-client";
import {
  createRecentRequestsScopeKey,
  shouldApplyRecentRequestsResult,
} from "../lib/hooks/use-recent-requests";
import { refreshWrapNonceState } from "../lib/hooks/use-wrapper-forms";
import { wrapperActionsTestHooks } from "../lib/hooks/use-wrapper-actions";
import { walletProviderTestHooks } from "../lib/wallet/provider";
import { metaMaskTestHooks } from "../lib/wallet/metamask";
import type { HistoryEntry } from "../components/dashboard-ui/types";
import { junoConfigTestHooks } from "../juno.config";

const TEST_CONFIG = {
  icHost: "https://icp-api.io",
  icpTokenListUrl: "/icp-token-list.sample.json",
  kasaneEvmCanisterId: "4c52m-aiaaa-aaaam-agwwa-cai",
  wrapCanisterId: "t63gs-up777-77776-aaaba-cai",
  evmWrapFactory: "0x88200f183e26d05bc6747ba7378cc73a68b6a12a",
  kasaneRpcUrl: "https://rpc-testnet.kasane.network",
  kasaneChainId: 4_801_360n,
  kasaneChainName: "Kasane",
  kasaneNativeCurrencySymbol: "ICP",
  kasaneBlockExplorerUrl: "https://explorer-testnet.kasane.network",
};

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
  assert.equal(
    Buffer.from(NATIVE_WITHDRAW_PRECOMPILE_ADDRESS).toString("hex"),
    "00000000000000000000000000000000ffff0002",
  );
  const nativePayload = encodeNativeWithdrawPayload({ recipient: "2vxsx-fae" });
  assert.equal(nativePayload.length, 31);
  assert.equal(nativePayload[0], 1);

  const wrapRequestId = deriveWrapRequestId({
    // 匿名principalではなく、実運用で使う非匿名principal bytesを使う。
    fromOwner: principalTextToBytes("4c52m-aiaaa-aaaam-agwwa-cai"),
    assetId: principalTextToBytes("2vxsx-fae"),
    amount: decimalToBytes32("1000000000000000000"),
    evmRecipient: hexToBytes("0x1111111111111111111111111111111111111111"),
    evmNonce: 1n,
    gasLimit: 300_000n,
  });
  const wrapRequestIdWithDifferentNonce = deriveWrapRequestId({
    fromOwner: principalTextToBytes("4c52m-aiaaa-aaaam-agwwa-cai"),
    assetId: principalTextToBytes("2vxsx-fae"),
    amount: decimalToBytes32("1000000000000000000"),
    evmRecipient: hexToBytes("0x1111111111111111111111111111111111111111"),
    evmNonce: 2n,
    gasLimit: 300_000n,
  });
  assert.equal(wrapRequestId.length, 32);
  assert.notDeepEqual(wrapRequestId, wrapRequestIdWithDifferentNonce);
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
    readRequest: async () => [{
      kind: { Unwrap: null },
      request_id: requestId,
      status: { Succeeded: null },
      error: [],
      fee_ledger_tx_id: [],
      pull_ledger_tx_id: [],
      mint_tx_id: [],
      withdraw_ledger_tx_id: [],
      ledger_tx_id: [Uint8Array.from([0xaa, 0xbb])],
      dispatch_status: [],
      dispatch_error: [],
      withdraw_error_code: [],
      charged_fee_e8s: [],
      charged_gas_price_wei: [],
    }],
  });
  assert.equal(unwrapOnly?.status, "Succeeded");
  assert.deepEqual(unwrapOnly?.ledgerTxId, Uint8Array.from([0xaa, 0xbb]));

  const wrapPreferred = await getExecutionResult(requestId, {
    readRequest: async () => [{
      kind: { Wrap: null },
      request_id: requestId,
      status: { Failed: null },
      error: [{ code: "wrap_failed", message: "wrap_failed" }],
      fee_ledger_tx_id: [],
      pull_ledger_tx_id: [Uint8Array.from([0x01])],
      mint_tx_id: [],
      withdraw_ledger_tx_id: [],
      ledger_tx_id: [],
      dispatch_status: [],
      dispatch_error: [],
      withdraw_error_code: ["withdraw_failed"],
      charged_fee_e8s: [],
      charged_gas_price_wei: [],
    }],
  });
  assert.equal(wrapPreferred?.errorCode, "wrap_failed");
  assert.equal(wrapPreferred?.mintFailedRecoverable, true);
  assert.equal(wrapPreferred?.withdrawErrorCode, "withdraw_failed");

  const missingWithdrawErrorCode = await getExecutionResult(requestId, {
    readRequest: async () => {
      const noError: [{ code: string; message: string }] = [{ code: "wrap_failed", message: "wrap_failed" }];
      const noBytes: [] = [];
      const noDispatchStatus: [] = [];
      const noText: [] = [];
      const noNat: [] = [];
      const pullLedgerTxId: [Uint8Array] = [Uint8Array.from([0x02])];
      const value = {
        kind: { Wrap: null },
        request_id: requestId,
        status: { Failed: null },
        error: noError,
        fee_ledger_tx_id: noBytes,
        pull_ledger_tx_id: pullLedgerTxId,
        mint_tx_id: noBytes,
        withdraw_ledger_tx_id: noBytes,
        withdraw_error_code: noBytes,
        ledger_tx_id: noBytes,
        dispatch_status: noDispatchStatus,
        dispatch_error: noText,
        charged_fee_e8s: noNat,
        charged_gas_price_wei: noNat,
      };
      Reflect.deleteProperty(value, "withdraw_error_code");
      return [value];
    },
  });
  assert.equal(missingWithdrawErrorCode?.withdrawErrorCode, null);
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
  assert.equal(formatE8sToIcpText4(14_216_140n), "0.1421");
  assert.equal(formatTokenBalance2(12_345_678n, 8), "0.12");
  assert.equal(formatTokenBalance2(123_456_789n, 6), "123.45");
  assert.equal(formatTokenBalance2(42n, 0), "42");
  assert.equal(formatTokenAmount(12_340_000n, 8), "0.1234");
  assert.equal(formatTokenAmount(123_000_000n, 6), "123");
  assert.equal(formatWeiToGwei2(1_727_419_315_967n), "1727");
}

async function runAllowanceTests(): Promise<void> {
  const separate = computeRequiredAllowances({
    assetLedgerCanister: "a",
    feeLedgerCanister: "b",
    amount: 200n,
    totalFeeE8s: 50n,
  });
  assert.equal(separate.requiredAssetAllowance, 200n);
  assert.equal(separate.requiredFeeAllowance, 1_000_053n);

  const merged = computeRequiredAllowances({
    assetLedgerCanister: "a",
    feeLedgerCanister: "a",
    amount: 200n,
    totalFeeE8s: 50n,
  });
  assert.equal(merged.requiredAssetAllowance, 1_000_253n);
  assert.equal(merged.requiredFeeAllowance, 0n);
}

async function runWrapInputValidationTests(): Promise<void> {
  assert.equal(parsePositiveU64("1", "validation.gas_limit.invalid"), 1n);
  assert.equal(parseTokenAmount("1.23", 8, "validation.amount.invalid"), 123_000_000n);
  assert.equal(parseTokenAmount("10", 6, "validation.amount.invalid"), 10_000_000n);
  assert.throws(
    () => parsePositiveU64("0", "validation.gas_limit.invalid"),
    /validation\.gas_limit\.invalid/,
  );
  assert.throws(
    () => parseTokenAmount("1.234", 2, "validation.amount.invalid"),
    /validation\.amount\.invalid/,
  );
}

async function runMetaMaskHelperTests(): Promise<void> {
  assert.equal(metaMaskTestHooks.normalizeChainIdHex(4_801_360n), "0x494350");
  assert.equal(
    metaMaskTestHooks.buildWalletAddEthereumChainParams({
      chainId: 4_801_360n,
      chainName: "Kasane",
      rpcUrl: "https://rpc-testnet.kasane.network",
      nativeCurrencySymbol: "ICP",
      blockExplorerUrl: "https://explorer-testnet.kasane.network",
    }).chainId,
    "0x494350",
  );
  assert.equal(
    metaMaskTestHooks.normalizeMetaMaskAddress("0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
    "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  );
  assert.deepEqual(
    metaMaskTestHooks.parseMetaMaskAccountsChanged(["0xabc"]),
    ["0xabc"],
  );
  assert.equal(
    metaMaskTestHooks.parseMetaMaskChainChanged("0x494350"),
    "0x494350",
  );
  assert.equal(
    metaMaskTestHooks.errorCodeOf({ code: 4902 }),
    4902,
  );
  assert.equal(
    metaMaskTestHooks.isUnknownChainError({ code: 4902 }),
    true,
  );
  assert.equal(
    metaMaskTestHooks.isUnknownChainError({ message: "wallet_switchEthereumChain failed with 4902" }),
    true,
  );
  assert.equal(
    metaMaskTestHooks.isUnknownChainError({ code: 4001, message: "user rejected" }),
    false,
  );
  assert.equal(
    walletProviderTestHooks.mapPrincipalToSession(null),
    null,
  );
  assert.equal(
    walletProviderTestHooks.resolveOisyPrincipalText([]),
    null,
  );
  assert.equal(
    walletProviderTestHooks.resolveOisyPrincipalText([{ owner: Principal.anonymous() }]),
    null,
  );
  assert.equal(
    walletProviderTestHooks.resolveOisyPrincipalText([{ owner: Principal.fromText("ryjl3-tyaaa-aaaaa-aaaba-cai") }]),
    "ryjl3-tyaaa-aaaaa-aaaba-cai",
  );
}

async function runPersistSubmittedRequestTests(): Promise<void> {
  const calls: string[] = [];
  wrapperActionsTestHooks.persistSubmittedRequest(async (entry) => {
    calls.push(entry.requestId);
    await Promise.resolve();
  }, {
    requestId: `0x${"56".repeat(32)}`,
    kind: "wrap",
    submittedAt: "2026-03-21T00:00:00.000Z",
  });
  await Promise.resolve();
  assert.deepEqual(calls, [`0x${"56".repeat(32)}`]);

  wrapperActionsTestHooks.persistSubmittedRequest(async () => {
    throw new Error("history.save_failed");
  }, {
    requestId: `0x${"78".repeat(32)}`,
    kind: "unwrap",
    submittedAt: "2026-03-21T00:00:00.000Z",
  });
  await Promise.resolve();
}

async function runFinishSubmittedUnwrapRequestTests(): Promise<void> {
  const requestIds: string[] = [];
  const saved: HistoryEntry[] = [];
  const messages: Array<string | null> = [];
  let resetCount = 0;
  let polledRequestId: string | null = null;

  await wrapperActionsTestHooks.finishSubmittedUnwrapRequest({
    requestIdHex: `0x${"34".repeat(32)}`,
    onRequestIdInput: (requestId) => {
      requestIds.push(requestId);
    },
    onRequestSubmitted: (entry) => {
      saved.push(entry);
    },
    startPollingSubmittedRequest: async (requestIdHex) => {
      polledRequestId = requestIdHex;
    },
    setMessage: (value) => {
      messages.push(value);
    },
    resetUnwrapNonceDeadline: () => {
      resetCount += 1;
    },
  });

  assert.deepEqual(requestIds, [`0x${"34".repeat(32)}`]);
  assert.equal(polledRequestId, `0x${"34".repeat(32)}`);
  assert.equal(saved.length, 1);
  assert.equal(saved[0]?.kind, "unwrap");
  assert.deepEqual(messages, ["submit.success"]);
  assert.equal(resetCount, 1);
}

function buildStubIdentity(principal: Principal): Identity {
  return {
    getPrincipal(): Principal {
      return principal;
    },
    async transformRequest(request) {
      return request;
    },
  };
}

async function runActorCacheTests(): Promise<void> {
  type QueryActor = { query: () => string };
  type SubmitActor = { submit: () => string };
  type Actor = QueryActor & SubmitActor;

  const cache = createActorCache<QueryActor, SubmitActor, Actor>();
  const createdActors: Actor[] = [];

  const queryActor = await cache.getQueryActor(async () => {
    const actor = {
      query: () => "query",
      submit: () => "submit",
    };
    createdActors.push(actor);
    return actor;
  });
  const cachedQueryActor = await cache.getQueryActor(async () => {
    throw new Error("query actor recreated");
  });
  assert.equal(queryActor, cachedQueryActor);
  assert.equal(createdActors.length, 1);

  const firstIdentity = buildStubIdentity(Principal.anonymous());
  const secondIdentity = buildStubIdentity(Principal.fromUint8Array(Uint8Array.from([1, 2, 3, 4])));
  const submitActor = await cache.getSubmitActor(firstIdentity, async () => {
    const actor = {
      query: () => "query",
      submit: () => "submit",
    };
    createdActors.push(actor);
    return actor;
  });
  const cachedSubmitActor = await cache.getSubmitActor(firstIdentity, async () => {
    throw new Error("submit actor recreated for same identity");
  });
  assert.equal(submitActor, cachedSubmitActor);

  const secondSubmitActor = await cache.getSubmitActor(secondIdentity, async () => {
    const actor = {
      query: () => "query-2",
      submit: () => "submit-2",
    };
    createdActors.push(actor);
    return actor;
  });
  assert.notEqual(submitActor, secondSubmitActor);

  const firstSignerActor = await cache.getSubmitActor({
    principalText: firstIdentity.getPrincipal().toText(),
    cacheKey: "oisy-session-1",
    identity: firstIdentity,
  }, async () => {
    const actor = {
      query: () => "query-3",
      submit: () => "submit-3",
    };
    createdActors.push(actor);
    return actor;
  });
  const cachedFirstSignerActor = await cache.getSubmitActor({
    principalText: firstIdentity.getPrincipal().toText(),
    cacheKey: "oisy-session-1",
    identity: firstIdentity,
  }, async () => {
    throw new Error("submit actor recreated for same signer session");
  });
  assert.equal(firstSignerActor, cachedFirstSignerActor);

  const secondSignerActor = await cache.getSubmitActor({
    principalText: firstIdentity.getPrincipal().toText(),
    cacheKey: "oisy-session-2",
    identity: firstIdentity,
  }, async () => {
    const actor = {
      query: () => "query-4",
      submit: () => "submit-4",
    };
    createdActors.push(actor);
    return actor;
  });
  assert.notEqual(firstSignerActor, secondSignerActor);

  cache.setMockQueryActor({ query: () => "mock-query" });
  cache.setMockSubmitActor({ submit: () => "mock-submit" });
  assert.equal((await cache.getQueryActor(async () => {
    throw new Error("query mock ignored");
  })).query(), "mock-query");
  assert.equal((await cache.getSubmitActor(firstIdentity, async () => {
    throw new Error("submit mock ignored");
  })).submit(), "mock-submit");

  cache.reset();
  const resetQueryActor = await cache.getQueryActor(async () => ({
    query: () => "query-reset",
    submit: () => "submit-reset",
  }));
  assert.equal(resetQueryActor.query(), "query-reset");
}

async function runWrapEstimateEncodingTests(): Promise<void> {
  const data = encodeFactoryMintForAssetCallData({
    assetId: principalTextToBytes("2vxsx-fae"),
    tokenDecimals: 8,
    evmRecipient: hexToBytes("0x1111111111111111111111111111111111111111"),
    amount: tokenAmountToBytes32("10000000000", 8),
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
  assert.equal(
    icrcClientTestHooks.decodeLedgerText([
      ["icrc1:name", { Text: "Internet Computer" }],
      ["icrc1:symbol", { Text: "ICP" }],
    ], "icrc1:symbol"),
    "ICP",
  );
  assert.equal(
    icrcClientTestHooks.decodeLedgerText([
      ["icrc1:symbol", { Blob: new Uint8Array([1, 2, 3]) }],
    ], "icrc1:symbol"),
    null,
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

async function runWrapClientSubmitTests(): Promise<void> {
  wrapClientTestHooks.reset();
  wrapClientTestHooks.setMockSubmitActor({
    submit_wrap_request: async (args) => {
      assert.equal(args.evm_nonce, 7n);
      assert.equal(args.max_fee_e8s, 9n);
      assert.equal(args.quoted_gas_price_wei, 11n);
      assert.equal(args.fee_ledger_canister.toText(), "2vxsx-fae");
      return {
        Ok: {
          request_id: Uint8Array.from([0x01]),
          charged_fee_e8s: 9n,
          charged_gas_price_wei: 11n,
          fee_ledger_tx_id: Uint8Array.from([0x02]),
        },
      };
    },
    retry_request: async () => {
      throw new Error("unused retry_request");
    },
    retry_native_deposit: async () => {
      throw new Error("unused retry_native_deposit");
    },
    retry_native_withdrawal: async () => {
      throw new Error("unused retry_native_withdrawal");
    },
    recover_failed_wrap: async () => {
      throw new Error("unused recover_failed_wrap");
    },
    submit_native_deposit: async () => {
      throw new Error("unused submit_native_deposit");
    },
  });
  const submitResult = await submitWrapRequest({
    assetId: "2vxsx-fae",
    amountE8s: 5n,
    evmRecipient: hexToBytes("0x1111111111111111111111111111111111111111"),
    evmNonce: 7n,
    gasLimit: 300_000n,
    maxFeeE8s: 9n,
    quotedGasPriceWei: 11n,
    feeLedgerCanister: "2vxsx-fae",
  }, new AnonymousIdentity());
  assert.deepEqual(submitResult.requestId, Uint8Array.from([0x01]));
  assert.equal(submitResult.chargedFeeE8s, 9n);
  wrapClientTestHooks.reset();
}

async function runWrapClientWithdrawErrorTests(): Promise<void> {
  wrapClientTestHooks.reset();
  wrapClientTestHooks.setMockSubmitActor({
    submit_wrap_request: async () => {
      throw new Error("unused submit_wrap_request");
    },
    retry_request: async () => {
      throw new Error("unused retry_request");
    },
    retry_native_deposit: async () => {
      throw new Error("unused retry_native_deposit");
    },
    retry_native_withdrawal: async () => {
      throw new Error("unused retry_native_withdrawal");
    },
    recover_failed_wrap: async () => ({
      Err: {
        InvalidArgument: {
          code: "withdraw.in_progress",
          message: "withdraw.in_progress",
        },
      },
    }),
    submit_native_deposit: async () => {
      throw new Error("unused submit_native_deposit");
    },
  });

  await assert.rejects(
    () => withdrawFailedWrap(Uint8Array.from([0x11]), new AnonymousIdentity()),
    /withdraw\.in_progress:withdraw\.in_progress/,
  );
  wrapClientTestHooks.reset();
}

async function runUnwrapRequirementsTests(): Promise<void> {
  wrapClientTestHooks.reset();
  wrapClientTestHooks.setMockQueryActor({
    get_request: async () => {
      throw new Error("unused get_request");
    },
    quote_wrap_request: async () => {
      throw new Error("unused quote_wrap_request");
    },
    get_native_deposit_result: async () => {
      throw new Error("unused get_native_deposit_result");
    },
    quote_native_deposit: async () => {
      throw new Error("unused quote_native_deposit");
    },
    quote_native_withdrawal: async () => {
      throw new Error("unused quote_native_withdrawal");
    },
    get_fee_policy: async () => {
      throw new Error("unused get_fee_policy");
    },
    get_unwrap_requirements: async () => ({
      Ok: {
        factory_address: hexToBytes("0x2222222222222222222222222222222222222222"),
        wrapped_token_address: [hexToBytes("0x3333333333333333333333333333333333333333")],
        balance: 1n,
        allowance: 9n,
        approve_required: false,
        readiness: { InsufficientBalance: null },
      },
    }),
  });
  const insufficientBalance = await getUnwrapRequirements({
    assetId: "2vxsx-fae",
    amountE8s: 5n,
    callerEvmAddress: hexToBytes("0x1111111111111111111111111111111111111111"),
  });
  assert.equal(insufficientBalance.readiness, "InsufficientBalance");
  assert.equal(insufficientBalance.approveRequired, false);

  wrapClientTestHooks.setMockQueryActor({
    get_request: async () => {
      throw new Error("unused get_request");
    },
    quote_wrap_request: async () => {
      throw new Error("unused quote_wrap_request");
    },
    get_native_deposit_result: async () => {
      throw new Error("unused get_native_deposit_result");
    },
    quote_native_deposit: async () => {
      throw new Error("unused quote_native_deposit");
    },
    quote_native_withdrawal: async () => {
      throw new Error("unused quote_native_withdrawal");
    },
    get_fee_policy: async () => {
      throw new Error("unused get_fee_policy");
    },
    get_unwrap_requirements: async () => ({
      Ok: {
        factory_address: hexToBytes("0x2222222222222222222222222222222222222222"),
        wrapped_token_address: [hexToBytes("0x3333333333333333333333333333333333333333")],
        balance: 9n,
        allowance: 1n,
        approve_required: true,
        readiness: { InsufficientAllowance: null },
      },
    }),
  });
  const insufficientAllowance = await getUnwrapRequirements({
    assetId: "2vxsx-fae",
    amountE8s: 5n,
    callerEvmAddress: hexToBytes("0x1111111111111111111111111111111111111111"),
  });
  assert.equal(insufficientAllowance.readiness, "InsufficientAllowance");
  assert.equal(insufficientAllowance.approveRequired, true);
  wrapClientTestHooks.reset();
}

async function runNativeWithdrawClientTests(): Promise<void> {
  wrapClientTestHooks.reset();
  wrapClientTestHooks.setMockQueryActor({
    get_request: async () => {
      throw new Error("unused get_request");
    },
    get_native_deposit_result: async () => {
      throw new Error("unused get_native_deposit_result");
    },
    quote_wrap_request: async () => {
      throw new Error("unused quote_wrap_request");
    },
    quote_native_deposit: async () => {
      throw new Error("unused quote_native_deposit");
    },
    quote_native_withdrawal: async (args) => {
      assert.equal(args.amount_e8s, 20_000n);
      assert.equal(args.recipient.toText(), "2vxsx-fae");
      return {
        Ok: {
          native_ledger_canister: Principal.fromText("2vxsx-fae"),
          ledger_fee_e8s: 10_000n,
          receive_amount_e8s: 10_000n,
        },
      };
    },
    get_fee_policy: async () => {
      throw new Error("unused get_fee_policy");
    },
    get_unwrap_requirements: async () => {
      throw new Error("unused get_unwrap_requirements");
    },
  });
  const quote = await quoteNativeWithdrawal({
    amountE8s: 20_000n,
    recipient: "2vxsx-fae",
  });
  assert.equal(quote.ledgerFeeE8s, 10_000n);
  assert.equal(quote.receiveAmountE8s, 10_000n);
  wrapClientTestHooks.reset();

  let txReadQuoteCalled = 0;
  const tx = await prepareNativeWithdrawTransaction({
    amountE8s: 20_001n,
    recipient: "2vxsx-fae",
    readQuote: async () => {
      txReadQuoteCalled += 1;
      return {
        nativeLedgerCanister: "2vxsx-fae",
        ledgerFeeE8s: 10_000n,
        receiveAmountE8s: 10_001n,
      };
    },
  });
  assert.equal(txReadQuoteCalled, 1);
  assert.equal(tx.to, "0x00000000000000000000000000000000ffff0002");
  assert.equal(tx.valueWei, 200_010_000_000_000n);
  assert.equal(tx.receiveAmountE8s, 10_001n);

  let sendCalled = false;
  await assert.rejects(
    async () => {
      await prepareNativeWithdrawTransaction({
        amountE8s: 10_000n,
        recipient: "2vxsx-fae",
        readQuote: async () => ({
          nativeLedgerCanister: "2vxsx-fae",
          ledgerFeeE8s: 10_000n,
          receiveAmountE8s: 0n,
        }),
      });
      sendCalled = true;
    },
    /native_withdraw\.amount_not_above_fee/,
  );
  assert.equal(sendCalled, false);
}

async function runApproveWrappedTokenTests(): Promise<void> {
  erc20ClientTestHooks.reset();
  erc20ClientTestHooks.setDeps({
    readRequirements: async () => ({
      factoryAddress: hexToBytes("0x2222222222222222222222222222222222222222"),
      wrappedTokenAddress: hexToBytes("0x3333333333333333333333333333333333333333"),
      balance: 1n,
      allowance: 9n,
      approveRequired: false,
      readiness: "InsufficientBalance",
    }),
    readExpectedNonce: async () => {
      throw new Error("nonce should not be requested");
    },
    readEstimateContractGasLimit: async () => {
      throw new Error("estimate should not run");
    },
    submitTx: async () => {
      throw new Error("submit should not run");
    },
  });
  await assert.rejects(
    () => approveWrappedTokenIfNeeded({
      assetId: "2vxsx-fae",
      amount: 5n,
      principalText: "4c52m-aiaaa-aaaam-agwwa-cai",
      caller: new AnonymousIdentity(),
    }),
    /erc20\.insufficient_balance/,
  );

  let approved = false;
  erc20ClientTestHooks.setDeps({
    readRequirements: async () => ({
      factoryAddress: hexToBytes("0x2222222222222222222222222222222222222222"),
      wrappedTokenAddress: hexToBytes("0x3333333333333333333333333333333333333333"),
      balance: 9n,
      allowance: 1n,
      approveRequired: true,
      readiness: "InsufficientAllowance",
    }),
    readExpectedNonce: async () => 7n,
    readEstimateContractGasLimit: async () => 99n,
    submitTx: async () => {
      approved = true;
      return Uint8Array.from([0xaa]);
    },
  });
  await approveWrappedTokenIfNeeded({
    assetId: "2vxsx-fae",
    amount: 5n,
    principalText: "4c52m-aiaaa-aaaam-agwwa-cai",
    caller: new AnonymousIdentity(),
  });
  assert.equal(approved, true);
  erc20ClientTestHooks.reset();
}

async function runWrapNonceRefreshTests(): Promise<void> {
  let state = "";
  let nonceText = "9";
  let errorText: string | null = "old";
  let readCalls = 0;

  await refreshWrapNonceState({
    walletPrincipalText: "4c52m-aiaaa-aaaam-agwwa-cai",
    wrapCanisterId: "lpuz5-uyaaa-aaaam-ah4da-cai",
    readWrapNonce: async () => {
      readCalls += 1;
      return 7n;
    },
    onIdle: () => {
      state = "idle";
      errorText = null;
      nonceText = "";
    },
    onLoading: () => {
      state = "loading";
      errorText = null;
    },
    onReady: (nonce) => {
      state = "ready";
      errorText = null;
      nonceText = nonce.toString();
    },
    onError: (message) => {
      state = "error";
      errorText = message;
      nonceText = "";
    },
    isCurrent: () => true,
  });
  assert.equal(readCalls, 1);
  assert.equal(state, "ready");
  assert.equal(errorText, null);
  assert.equal(nonceText, "7");

  await refreshWrapNonceState({
    walletPrincipalText: "4c52m-aiaaa-aaaam-agwwa-cai",
    wrapCanisterId: "lpuz5-uyaaa-aaaam-ah4da-cai",
    readWrapNonce: async () => {
      throw new Error("wrap.nonce_failed");
    },
    onIdle: () => {
      state = "idle";
      errorText = null;
      nonceText = "";
    },
    onLoading: () => {
      state = "loading";
      errorText = null;
    },
    onReady: (nonce) => {
      state = "ready";
      errorText = null;
      nonceText = nonce.toString();
    },
    onError: (message) => {
      state = "error";
      errorText = message;
      nonceText = "";
    },
    isCurrent: () => true,
  });
  assert.equal(state, "error");
  assert.equal(errorText, "wrap.nonce_failed");
  assert.equal(nonceText, "");
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
  ], "https://icp-api.io");
  assert.ok(merged.length >= 5);
  assert.equal(merged.filter((asset) => asset.assetId === "2vxsx-fae").length, 1);
  assert.ok(
    merged.some(
      (asset) =>
        asset.assetId === DEFAULT_ASSET_ID && asset.label === "ICP",
    ),
  );
  assert.equal(DEFAULT_ASSET_ID, "ryjl3-tyaaa-aaaaa-aaaba-cai");

  assert.deepEqual(
    presetAssetOptions("https://icp-api.io").map((asset) => asset.assetId),
    [
      "ryjl3-tyaaa-aaaaa-aaaba-cai",
      "mxzaz-hqaaa-aaaar-qaada-cai",
      "ss2fx-dyaaa-aaaar-qacoq-cai",
      "xevnm-gaaaa-aaaar-qafnq-cai",
    ],
  );
  assert.deepEqual(
    presetAssetOptions("http://127.0.0.1:8000").map((asset) => asset.assetId),
    [
      LOCAL_TEST_ASSET_ID,
      "ryjl3-tyaaa-aaaaa-aaaba-cai",
      "mxzaz-hqaaa-aaaar-qaada-cai",
      "ss2fx-dyaaa-aaaar-qacoq-cai",
      "xevnm-gaaaa-aaaar-qafnq-cai",
    ],
  );
  assert.equal(
    presetAssetOptions("http://127.0.0.1:8000")[0]?.label,
    "TESTICP",
  );
  assert.equal(assetCatalogTestHooks.usesLocalIcHost("https://icp-api.io"), false);
  assert.equal(assetCatalogTestHooks.usesLocalIcHost("http://localhost:8000"), true);
  assert.equal(assetCatalogTestHooks.resolveLedgerQueryHost(LOCAL_TEST_ASSET_ID, "http://127.0.0.1:8000"), "http://127.0.0.1:8000");
  assert.equal(assetCatalogTestHooks.resolveLedgerQueryHost(DEFAULT_ASSET_ID, "http://127.0.0.1:8000"), MAINNET_IC_HOST);
  assert.equal(assetCatalogTestHooks.resolveLedgerQueryHost(DEFAULT_ASSET_ID, "https://icp-api.io"), "https://icp-api.io");

  const serialized = serializeCustomAssets([custom]);
  const parsed = parseStoredCustomAssets(serialized);
  assert.deepEqual(parsed, [custom]);
  assert.deepEqual(dedupeAssetOptions([custom, custom]), [custom]);
}

async function runManageTokenListTests(): Promise<void> {
  const rows = normalizeIcpTokenList({
    content: [
      {
        ledgerId: "ryjl3-tyaaa-aaaaa-aaaba-cai",
        symbol: "ICP",
        name: "Internet Computer",
        logo: "https://example.com/icp.png",
      },
      {
        ledgerId: "mxzaz-hqaaa-aaaar-qaada-cai",
        symbol: "ckBTC",
        name: "Chain-key Bitcoin",
      },
      {
        ledgerId: "mxzaz-hqaaa-aaaar-qaada-cai",
        symbol: "duplicate",
      },
    ],
  });
  assert.equal(rows.length, 2);
  assert.equal(rows[0]?.assetId, "ryjl3-tyaaa-aaaaa-aaaba-cai");
  assert.match(rows[0]?.searchText ?? "", /internet computer/);
  assert.equal(rows[0]?.balanceText, null);
  assert.equal(toManageTokenOptions(rows)[0]?.source, "token_list");
  assert.throws(() => normalizeIcpTokenList({ content: [{ symbol: "ICP" }] }), /token_list\.asset_id_missing/);

  const updatedWrap = applySelectedAsset({
    tab: "wrap",
    assetId: "mxzaz-hqaaa-aaaar-qaada-cai",
    wrapForm: {
      assetId: DEFAULT_ASSET_ID,
      amount: "1",
      evmRecipient: "0x11",
      evmNonce: "1",
      gasLimit: "300000",
    },
    unwrapForm: {
      assetId: DEFAULT_ASSET_ID,
      amount: "2",
      recipient: "aaaaa-aa",
    },
  });
  assert.equal(updatedWrap.wrapForm.assetId, "mxzaz-hqaaa-aaaar-qaada-cai");
  assert.equal(updatedWrap.unwrapForm.assetId, DEFAULT_ASSET_ID);

  const updatedUnwrap = applySelectedAsset({
    tab: "unwrap",
    assetId: "ss2fx-dyaaa-aaaar-qacoq-cai",
    wrapForm: updatedWrap.wrapForm,
    unwrapForm: updatedWrap.unwrapForm,
  });
  assert.equal(updatedUnwrap.wrapForm.assetId, "mxzaz-hqaaa-aaaar-qaada-cai");
  assert.equal(updatedUnwrap.unwrapForm.assetId, "ss2fx-dyaaa-aaaar-qacoq-cai");
}

async function runDevelopmentTokenListFixtureTests(): Promise<void> {
  const payload = JSON.parse(
    readFileSync(new URL("../public/icp-token-list.development.json", import.meta.url), "utf8"),
  ) as Array<{
    ledgerId?: string;
    symbol?: string;
    logo?: string | null;
  }>;
  assert.ok(
    payload.some(
      (row) => row.ledgerId === LOCAL_TEST_ASSET_ID && row.symbol === "TESTICP" && typeof row.logo === "string",
    ),
  );
}

async function runInternetIdentityConfigTests(): Promise<void> {
  const testEnvBase: NodeJS.ProcessEnv = {
    NODE_ENV: "test",
  };
  assert.throws(
    () => configTestHooks.loadConfigFromEnv(testEnvBase),
    /config\.missing:VITE_IC_HOST/,
  );
  assert.throws(
    () => configTestHooks.loadConfigFromEnv({
      ...testEnvBase,
      VITE_IC_HOST: "http://127.0.0.1:8000",
    }),
    /config\.missing:VITE_ICP_TOKEN_LIST_URL/,
  );
  assert.throws(
    () => configTestHooks.loadConfigFromEnv({
      ...testEnvBase,
      VITE_IC_HOST: "http://127.0.0.1:8000",
      VITE_ICP_TOKEN_LIST_URL: "/icp-token-list.sample.json",
    }),
    /config\.missing:VITE_KASANE_EVM_CANISTER_ID/,
  );
  assert.throws(
    () => configTestHooks.loadConfigFromEnv({
      ...testEnvBase,
      VITE_IC_HOST: "http://127.0.0.1:8000",
      VITE_ICP_TOKEN_LIST_URL: "/icp-token-list.sample.json",
      VITE_KASANE_EVM_CANISTER_ID: "4c52m-aiaaa-aaaam-agwwa-cai",
    }),
    /config\.missing:VITE_WRAP_CANISTER_ID/,
  );
  assert.throws(
    () => configTestHooks.loadConfigFromEnv({
      ...testEnvBase,
      VITE_IC_HOST: "http://127.0.0.1:8000",
      VITE_ICP_TOKEN_LIST_URL: "/icp-token-list.sample.json",
      VITE_KASANE_EVM_CANISTER_ID: "4c52m-aiaaa-aaaam-agwwa-cai",
      VITE_WRAP_CANISTER_ID: "t63gs-up777-77776-aaaba-cai",
    }),
    /config\.missing:VITE_EVM_WRAP_FACTORY/,
  );
  assert.deepEqual(
    configTestHooks.loadConfigFromEnv({
      ...testEnvBase,
      VITE_IC_HOST: "http://127.0.0.1:8000",
      VITE_ICP_TOKEN_LIST_URL: "/icp-token-list.sample.json",
      VITE_KASANE_EVM_CANISTER_ID: "4c52m-aiaaa-aaaam-agwwa-cai",
      VITE_WRAP_CANISTER_ID: "t63gs-up777-77776-aaaba-cai",
      VITE_EVM_WRAP_FACTORY: "0x88200f183e26d05bc6747ba7378cc73a68b6a12a",
      VITE_KASANE_RPC_URL: "https://rpc-testnet.kasane.network",
      VITE_KASANE_CHAIN_ID: "4801360",
      VITE_KASANE_CHAIN_NAME: "Kasane",
      VITE_KASANE_NATIVE_CURRENCY_SYMBOL: "ICP",
      VITE_KASANE_BLOCK_EXPLORER_URL: "https://explorer-testnet.kasane.network",
    }),
    {
      icHost: "http://127.0.0.1:8000",
      icpTokenListUrl: "/icp-token-list.sample.json",
      kasaneEvmCanisterId: "4c52m-aiaaa-aaaam-agwwa-cai",
      wrapCanisterId: "t63gs-up777-77776-aaaba-cai",
      evmWrapFactory: "0x88200f183e26d05bc6747ba7378cc73a68b6a12a",
      kasaneRpcUrl: "https://rpc-testnet.kasane.network",
      kasaneChainId: 4_801_360n,
      kasaneChainName: "Kasane",
      kasaneNativeCurrencySymbol: "ICP",
      kasaneBlockExplorerUrl: "https://explorer-testnet.kasane.network",
    },
  );
  assert.equal(
    configTestHooks.resolveConfiguredIdentityProviderFromEnv({
      ...process.env,
      VITE_INTERNET_IDENTITY_URL: "",
    }),
    null,
  );
  assert.equal(
    configTestHooks.resolveConfiguredIdentityProviderFromEnv({
      ...process.env,
      VITE_INTERNET_IDENTITY_URL: "http://rdmx6-jaaaa-aaaaa-aaadq-cai.localhost:8000",
    }),
    "http://rdmx6-jaaaa-aaaaa-aaadq-cai.localhost:8000",
  );
  assert.equal(
    configTestHooks.resolveConfiguredInternetIdentityDomainFromEnv({
      VITE_INTERNET_IDENTITY_URL: "https://identity.ic0.app",
    }),
    "ic0.app",
  );
  assert.equal(
    configTestHooks.resolveConfiguredInternetIdentityDomainFromEnv({
      VITE_INTERNET_IDENTITY_URL: "https://identity.internetcomputer.org",
    }),
    "internetcomputer.org",
  );
  assert.equal(
    configTestHooks.resolveConfiguredInternetIdentityDomainFromEnv({
      VITE_INTERNET_IDENTITY_URL: "https://identity.id.ai",
    }),
    "id.ai",
  );
  assert.equal(
    configTestHooks.resolveConfiguredInternetIdentityDomainFromEnv({
      VITE_INTERNET_IDENTITY_URL: "http://127.0.0.1:4943",
    }),
    null,
  );
  assert.equal(
    configTestHooks.resolveConfiguredDerivationOriginFromEnv({
      ...process.env,
      VITE_II_DERIVATION_ORIGIN: "",
    }),
    null,
  );
  assert.equal(
    configTestHooks.resolveConfiguredDerivationOriginFromEnv({
      ...process.env,
      VITE_II_DERIVATION_ORIGIN: "https://wrap.example.com",
    }),
    "https://wrap.example.com",
  );
  assert.equal(configTestHooks.shouldFetchRootKey("http://127.0.0.1:8000"), true);
  assert.equal(configTestHooks.shouldFetchRootKey("http://localhost:8000"), true);
  assert.equal(configTestHooks.shouldFetchRootKey("https://icp-api.io"), false);
  assert.equal(configTestHooks.parseChainId("4801360"), 4_801_360n);
  assert.equal(
    configTestHooks.resolveJunoSatelliteIdFromEnv({
      ...process.env,
      VITE_JUNO_SATELLITE_ID: "",
    }),
    null,
  );
  assert.equal(
    configTestHooks.resolveJunoSatelliteIdFromEnv({
      ...process.env,
      VITE_JUNO_SATELLITE_ID: "mxzaz-hqaaa-aaaal-qsdoa-cai",
    }),
    "mxzaz-hqaaa-aaaal-qsdoa-cai",
  );
  assert.equal(typeof loadConfig, "function");
}

async function runJunoConfigTests(): Promise<void> {
  assert.deepEqual(
    junoConfigTestHooks.parseConfiguredAllowedTargets(undefined),
    [],
  );
  assert.deepEqual(
    junoConfigTestHooks.parseConfiguredAllowedTargets(" ,  mxzaz-hqaaa-aaaar-qaada-cai ,, xevnm-gaaaa-aaaar-qafnq-cai  "),
    ["mxzaz-hqaaa-aaaar-qaada-cai", "xevnm-gaaaa-aaaar-qafnq-cai"],
  );
  assert.deepEqual(
    junoConfigTestHooks.parseConfiguredAllowedTargets("mxzaz-hqaaa-aaaar-qaada-cai, mxzaz-hqaaa-aaaar-qaada-cai"),
    ["mxzaz-hqaaa-aaaar-qaada-cai"],
  );
  assert.throws(
    () => junoConfigTestHooks.parseConfiguredAllowedTargets("not-a-principal"),
  );
  assert.equal(junoConfigTestHooks.usesLocalIcHost({ VITE_IC_HOST: "https://icp-api.io" }), false);
  assert.equal(junoConfigTestHooks.usesLocalIcHost({ VITE_IC_HOST: "http://127.0.0.1:8000" }), true);

  assert.deepEqual(
    junoConfigTestHooks.resolveAllowedTargets({
      VITE_IC_HOST: "https://icp-api.io",
      VITE_WRAP_CANISTER_ID: "t63gs-up777-77776-aaaba-cai",
      VITE_KASANE_EVM_CANISTER_ID: "4c52m-aiaaa-aaaam-agwwa-cai",
    }),
    [
      "t63gs-up777-77776-aaaba-cai",
      "4c52m-aiaaa-aaaam-agwwa-cai",
      "ryjl3-tyaaa-aaaaa-aaaba-cai",
      "mxzaz-hqaaa-aaaar-qaada-cai",
      "ss2fx-dyaaa-aaaar-qacoq-cai",
      "xevnm-gaaaa-aaaar-qafnq-cai",
    ],
  );
  assert.deepEqual(
    junoConfigTestHooks.resolveAllowedTargets({
      VITE_IC_HOST: "http://127.0.0.1:8000",
      VITE_WRAP_CANISTER_ID: "t63gs-up777-77776-aaaba-cai",
      VITE_KASANE_EVM_CANISTER_ID: "4c52m-aiaaa-aaaam-agwwa-cai",
    }),
    [
      "t63gs-up777-77776-aaaba-cai",
      "4c52m-aiaaa-aaaam-agwwa-cai",
      "ryjl3-tyaaa-aaaaa-aaaba-cai",
      "mxzaz-hqaaa-aaaar-qaada-cai",
      "ss2fx-dyaaa-aaaar-qacoq-cai",
      "xevnm-gaaaa-aaaar-qafnq-cai",
      "xafvr-biaaa-aaaai-aql5q-cai",
    ],
  );
  assert.deepEqual(
    junoConfigTestHooks.resolveAllowedTargets({
      VITE_IC_HOST: "https://icp-api.io",
      VITE_WRAP_CANISTER_ID: "t63gs-up777-77776-aaaba-cai",
      VITE_KASANE_EVM_CANISTER_ID: "4c52m-aiaaa-aaaam-agwwa-cai",
      JUNO_AUTH_ALLOWED_TARGETS:
        " xevnm-gaaaa-aaaar-qafnq-cai, 2vxsx-fae , t63gs-up777-77776-aaaba-cai ",
    }),
    [
      "t63gs-up777-77776-aaaba-cai",
      "4c52m-aiaaa-aaaam-agwwa-cai",
      "ryjl3-tyaaa-aaaaa-aaaba-cai",
      "mxzaz-hqaaa-aaaar-qaada-cai",
      "ss2fx-dyaaa-aaaar-qacoq-cai",
      "xevnm-gaaaa-aaaar-qafnq-cai",
      "2vxsx-fae",
    ],
  );
}

async function runAgentConfigInjectionTests(): Promise<void> {
  resetAgentCache();
  const queryAgent = await getQueryAgent({
    loadConfig: () => TEST_CONFIG,
  });
  assert.equal(queryAgent.config.host, "https://icp-api.io");

  resetAgentCache();
  const identityAgent = await getIdentityAgent(new AnonymousIdentity(), {
    loadConfig: () => TEST_CONFIG,
  });
  assert.equal(identityAgent.config.host, "https://icp-api.io");
  resetAgentCache();
}

async function runRecentRequestsTests(): Promise<void> {
  const principalText = "aaaaa-aa";
  const first: HistoryEntry = {
    requestId: `0x${"11".repeat(32)}`,
    kind: "wrap",
    submittedAt: "2026-03-18T12:00:00.000Z",
  };
  const second: HistoryEntry = {
    requestId: `0x${"22".repeat(32)}`,
    kind: "unwrap",
    submittedAt: "2026-03-18T12:01:00.000Z",
  };

  assert.equal(
    createRecentRequestKey(principalText, first.requestId),
    `${principalText}:${first.requestId}`,
  );
  assert.throws(
    () => createRecentRequestKey(principalText, "0x1234"),
    /history\.request_id_invalid/,
  );

  assert.deepEqual(
    RecentRequestDocSchema.parse({
      principalText,
      requestId: first.requestId,
      kind: "wrap",
      submittedAt: first.submittedAt,
    }),
    {
      principalText,
      requestId: first.requestId,
      kind: "wrap",
      submittedAt: first.submittedAt,
    },
  );
  assert.throws(
    () => RecentRequestDocSchema.parse({
      principalText,
      requestId: first.requestId,
      kind: "invalid",
      submittedAt: "",
    }),
    /history\./,
  );

  const deduped = mergeRecentRequestHistory([first, second], {
    ...first,
    submittedAt: "2026-03-18T12:02:00.000Z",
  });
  assert.deepEqual(deduped, [
    {
      ...first,
      submittedAt: "2026-03-18T12:02:00.000Z",
    },
    second,
  ]);

  const limited = mergeRecentRequestHistory([first, second], {
    requestId: `0x${"33".repeat(32)}`,
    kind: "wrap",
    submittedAt: "2026-03-18T12:03:00.000Z",
  }, 2);
  assert.deepEqual(limited, [
    {
      requestId: `0x${"33".repeat(32)}`,
      kind: "wrap",
      submittedAt: "2026-03-18T12:03:00.000Z",
    },
    first,
  ]);

  const satelliteId = "mxzaz-hqaaa-aaaal-qsdoa-cai";
  const actorKey = recentRequestsClientTestHooks.createRecentRequestsActorCacheKey(
    {
      principalText: "2vxsx-fae",
      identity: new AnonymousIdentity(),
    },
    satelliteId,
  );
  assert.equal(actorKey, `2vxsx-fae:${satelliteId}`);

  const scopeA = createRecentRequestsScopeKey({
    principalText,
    satelliteId,
  });
  const scopeB = createRecentRequestsScopeKey({
    principalText: "bbbbb-bb",
    satelliteId,
  });
  const signedOutScope = createRecentRequestsScopeKey({
    principalText: null,
    satelliteId,
  });
  assert.equal(
    shouldApplyRecentRequestsResult({
      startedScopeKey: scopeA,
      currentScopeKey: scopeA,
      startedRefreshSeq: 4,
      currentRefreshSeq: 4,
    }),
    true,
  );
  assert.equal(
    shouldApplyRecentRequestsResult({
      startedScopeKey: scopeA,
      currentScopeKey: scopeB,
      startedRefreshSeq: 4,
      currentRefreshSeq: 4,
    }),
    false,
  );
  assert.equal(
    shouldApplyRecentRequestsResult({
      startedScopeKey: scopeA,
      currentScopeKey: signedOutScope,
      startedRefreshSeq: 4,
      currentRefreshSeq: 5,
    }),
    false,
  );
  assert.equal(
    shouldApplyRecentRequestsResult({
      startedScopeKey: scopeA,
      currentScopeKey: scopeA,
    }),
    true,
  );
  assert.equal(
    shouldApplyRecentRequestsResult({
      startedScopeKey: scopeA,
      currentScopeKey: scopeB,
    }),
    false,
  );
  recentRequestsClientTestHooks.reset();
}

async function runClientDepsResetTests(): Promise<void> {
  wrapperClientTestHooks.setDeps({
    loadConfig: () => ({
      ...TEST_CONFIG,
      icHost: "https://example.com",
      kasaneEvmCanisterId: "aaaaa-aa",
      wrapCanisterId: "aaaaa-aa",
      evmWrapFactory: "0x1",
    }),
  });
  wrapClientTestHooks.setDeps({
    loadConfig: () => ({
      ...TEST_CONFIG,
      icHost: "https://example.com",
      kasaneEvmCanisterId: "aaaaa-aa",
      wrapCanisterId: "aaaaa-aa",
      evmWrapFactory: "0x1",
    }),
  });
  wrapperClientTestHooks.reset();
  wrapClientTestHooks.reset();
}

async function runStatusPhaseTests(): Promise<void> {
  assert.equal(deriveStatusPhase(null), "idle");
  assert.equal(
    deriveStatusPhase({
      kind: "request",
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
      kind: "request",
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
      kind: "request",
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
      kind: "request",
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
  const nonTerminalStatus: StatusResponse = {
    kind: "request",
    requestId: "0x11",
    dispatchStatus: "Dispatching",
    executionStatus: "Running",
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
    estimate_ic_tx: async () => ({
      Ok: {
        gas_limit: 300_000n,
        suggested_max_fee_per_gas: 1n,
        suggested_max_priority_fee_per_gas: 1n,
      },
    }),
    get_unwrap_request_ids_by_eth_tx_hash: async () => [],
    get_unwrap_request_ids_by_tx_id: async () => [],
    get_unwrap_dispatch_overview: async (): Promise<[]> => [],
  };
}

async function runGetGatewayRequestLookupTests(): Promise<void> {
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
    get_unwrap_request_ids_by_eth_tx_hash: async (ethTxHash) => [
      Uint8Array.from(ethTxHash),
      Uint8Array.from(new Array(32).fill(0x33)),
    ],
    get_unwrap_dispatch_overview: async (requestId) => [{
      request_id: requestId,
      status: { Dispatched: null },
      error: [],
    }],
  });

  const ids = await getUnwrapRequestIdsByTxId(Uint8Array.from(new Array(32).fill(0x11)));
  assert.equal(ids.length, 2);
  assert.deepEqual(ids[1], Uint8Array.from(new Array(32).fill(0x22)));

  const ethHashIds = await getUnwrapRequestIdsByEthTxHash(Uint8Array.from(new Array(32).fill(0x44)));
  assert.equal(ethHashIds.length, 2);
  assert.deepEqual(ethHashIds[1], Uint8Array.from(new Array(32).fill(0x33)));

  const out = await getDispatchResult(Uint8Array.from(new Array(32).fill(0x11)));
  assert.equal(out?.status, "Dispatched");
  assert.equal(out?.errorCode, null);
}

async function runWrapperClientFeeTests(): Promise<void> {
  wrapperClientTestHooks.reset();
  const submittedArgsList: Array<{
    to: [] | [Uint8Array];
    from: [] | [Uint8Array];
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
    caller: new AnonymousIdentity(),
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
      caller: new AnonymousIdentity(),
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
      caller: new AnonymousIdentity(),
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
  await runMetaMaskHelperTests();
  await runPersistSubmittedRequestTests();
  await runFinishSubmittedUnwrapRequestTests();
  await runActorCacheTests();
  await runWrapEstimateEncodingTests();
  await runErc20EncodingTests();
  await runLedgerMetadataTests();
  await runEstimateWrapGasClientTests();
  await runEstimateUnwrapGasClientTests();
  await runWrapClientSubmitTests();
  await runWrapClientWithdrawErrorTests();
  await runUnwrapRequirementsTests();
  await runNativeWithdrawClientTests();
  await runApproveWrappedTokenTests();
  await runWrapNonceRefreshTests();
  await runWrapNonceClientTests();
  await runAssetCatalogTests();
  await runManageTokenListTests();
  await runDevelopmentTokenListFixtureTests();
  await runInternetIdentityConfigTests();
  await runJunoConfigTests();
  await runAgentConfigInjectionTests();
  await runRecentRequestsTests();
  await runClientDepsResetTests();
  await runStatusPhaseTests();
  await runStatusPollingRegressionTests();
  await runGetGatewayRequestLookupTests();
  await runWrapperClientFeeTests();
  process.stdout.write("wrapper tests passed\n");
}

main().catch((error: unknown) => {
  process.stderr.write(`${error instanceof Error ? error.stack ?? error.message : String(error)}\n`);
  process.exitCode = 1;
});
