import assert from "node:assert/strict";
import { AnonymousIdentity } from "@icp-sdk/core/agent";
import { Principal } from "@icp-sdk/core/principal";
import { renderToStaticMarkup } from "react-dom/server";
import { RequestStatusModal } from "../components/dashboard-ui/request-status-modal";
import { wrapperActionsTestHooks } from "../lib/hooks/use-wrapper-actions";
import { getExecutionResult } from "../lib/canister/wrap-client";
import { mergeStatus } from "../lib/merge";
import { computeRequiredAllowances } from "../lib/wrap-flow";
import type { StatusResponse } from "../lib/types";
import type { RequestOverview } from "../src/declarations/evm_canister/evm_canister.did";

export async function runBridgeRecoveryTests(): Promise<void> {
  const caller = { identity: new AnonymousIdentity(), principalText: "2vxsx-fae" };
  const requestId = `0x${"12".repeat(32)}`;
  const calls: string[] = [];
  let allowance = 0n;
  let approved = 0n;
  const retryId = new Uint8Array(32).fill(0x12);
  const deps = {
    getLedgerFee: async () => {
      calls.push("fee");
      return 10n;
    },
    getLedgerAllowance: async () => {
      calls.push("allowance");
      return allowance;
    },
    approveLedgerSpend: async (args: { amount: bigint }) => {
      calls.push("approve");
      approved = args.amount;
      return 1n;
    },
    retryFailedWrap: async () => {
      calls.push("wrap");
      return retryId;
    },
    retryNativeDeposit: async () => {
      calls.push("native");
      return retryId;
    },
    retryFailedUnwrap: async () => {
      calls.push("unwrap");
      return retryId;
    },
  };
  const status: StatusResponse = {
    kind: "request",
    requestId,
    requestKind: "Wrap",
    recoveryAction: "RetryWrap",
    retryAsset: { assetId: "aaaaa-aa", amount: 200n, caller: caller.principalText },
    executionStatus: "Failed",
    dispatchStatus: null,
    ledgerTxId: null,
    errorCode: null,
    mintFailedRecoverable: false,
    withdrawn: false,
    withdrawLedgerTxId: null,
    withdrawErrorCode: null,
  };
  const args = {
    status,
    caller,
    principalText: caller.principalText,
    spenderCanisterId: "aaaaa-aa",
  };
  await wrapperActionsTestHooks.retryBridgeRequest(args, deps);
  assert.deepEqual(calls, ["fee", "allowance", "approve", "wrap"]);
  assert.equal(approved, 210n);
  calls.length = 0;
  allowance = 210n;
  await wrapperActionsTestHooks.retryBridgeRequest(args, deps);
  assert.deepEqual(calls, ["fee", "allowance", "wrap"]);
  calls.length = 0;
  await assert.rejects(
    wrapperActionsTestHooks.retryBridgeRequest(args, {
      ...deps,
      getLedgerFee: async () => {
        throw new Error("fee unavailable");
      },
    }),
    /fee unavailable/,
  );
  assert.deepEqual(calls, []);
  await assert.rejects(
    wrapperActionsTestHooks.retryBridgeRequest({ ...args, principalText: "aaaaa-aa" }, deps),
    /unauthorized/,
  );
  assert.deepEqual(calls, []);
  allowance = 0n;
  await assert.rejects(
    wrapperActionsTestHooks.retryBridgeRequest(args, {
      ...deps,
      approveLedgerSpend: async () => {
        throw new Error("approval rejected");
      },
    }),
    /approval rejected/,
  );
  assert.deepEqual(calls, ["fee", "allowance"]);
  calls.length = 0;
  await wrapperActionsTestHooks.retryBridgeRequest(
    {
      ...args,
      status: {
        ...status,
        requestKind: "NativeDeposit",
        recoveryAction: "RetryNativeDeposit",
        retryAsset: null,
      },
    },
    deps,
  );
  assert.deepEqual(calls, ["native"]);
  calls.length = 0;
  await assert.rejects(
    wrapperActionsTestHooks.retryBridgeRequest(
      { ...args, status: { ...status, recoveryAction: null } },
      deps,
    ),
    /retry_invalid_state/,
  );
  assert.deepEqual(calls, []);

  for (const [action, kind, label] of [
    ["RetryWrap", "Wrap", "Retry Failed Wrap"],
    ["RetryNativeDeposit", "NativeDeposit", "Retry Native Deposit"],
    ["RefundWrap", "Wrap", "Withdraw Failed Wrap"],
  ] as const) {
    const html = renderToStaticMarkup(
      <RequestStatusModal
        open
        requestIdLabel={requestId}
        status={{ ...status, recoveryAction: action, requestKind: kind }}
        statusLoading={false}
        message={null}
        walletConnected
        retryLoading={false}
        withdrawLoading={false}
        onClose={() => {}}
        onRetry={() => {}}
        onWithdraw={() => {}}
      />,
    );
    assert.ok(html.includes(label));
    if (kind === "NativeDeposit") assert.ok(!html.includes("Withdraw Failed Wrap"));
  }
  const blocked = renderToStaticMarkup(
    <RequestStatusModal
      open
      requestIdLabel={requestId}
      status={{ ...status, recoveryAction: null, mintFailedRecoverable: true }}
      statusLoading={false}
      message={null}
      walletConnected
      retryLoading={false}
      withdrawLoading={false}
      onClose={() => {}}
      onRetry={() => {}}
      onWithdraw={() => {}}
    />,
  );
  assert.ok(!blocked.includes("Withdraw Failed Wrap"));

  const wire: RequestOverview = {
    kind: { NativeDeposit: null },
    request_id: retryId,
    status: { Failed: null },
    stage: [],
    recovery_action: [{ RetryNativeDeposit: null }],
    retry_asset: [],
    recoverable: true,
    error: [],
    fee_ledger_tx_id: [],
    pull_ledger_tx_id: [new Uint8Array([1])],
    mint_tx_id: [],
    withdraw_ledger_tx_id: [],
    withdrawn: false,
    withdraw_in_progress: false,
    withdraw_error: [],
    ledger_tx_id: [],
    dispatch_status: [],
    dispatch_error: [],
    charged_fee_e8s: [],
    charged_gas_price_wei: [],
  };
  const result = await getExecutionResult(retryId, { readRequest: async () => [wire] });
  const merged = mergeStatus({
    requestIdHex: requestId,
    executionResult: result,
    dispatchResult: null,
  });
  assert.equal(merged.requestKind, "NativeDeposit");
  assert.equal(merged.recoveryAction, "RetryNativeDeposit");
  wire.kind = { Wrap: null };
  wire.recovery_action = [{ RetryWrap: null }];
  wire.retry_asset = [
    {
      asset_id: Principal.fromText("aaaaa-aa"),
      amount: 200n,
      caller: Principal.fromText(caller.principalText),
    },
  ];
  const wrap = await getExecutionResult(retryId, { readRequest: async () => [wire] });
  assert.deepEqual(wrap?.retryAsset, status.retryAsset);

  const quote = {
    assetLedgerCanister: "a",
    feeLedgerCanister: "b",
    amount: 200n,
    totalFeeE8s: 50n,
  };
  assert.equal(
    computeRequiredAllowances({ ...quote, assetTransferFee: 0n, feeTransferFee: 0n })
      .requiredAssetAllowance,
    200n,
  );
  assert.deepEqual(
    computeRequiredAllowances({ ...quote, assetTransferFee: 10n, feeTransferFee: 2_000_000n }),
    { requiredAssetAllowance: 210n, requiredFeeAllowance: 2_000_050n },
  );
  assert.deepEqual(
    computeRequiredAllowances({
      ...quote,
      feeLedgerCanister: "a",
      assetTransferFee: 2_000_000n,
      feeTransferFee: 2_000_000n,
    }),
    { requiredAssetAllowance: 4_000_250n, requiredFeeAllowance: 0n },
  );
}
