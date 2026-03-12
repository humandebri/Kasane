// どこで: wrapper の EVM token/factory client
// 何を: unwrap 前の token 解決・allowance 確認・approve 送信を提供
// なぜ: canister の preflight API を使って unwrap 前提を揃えるため

import type { Identity } from "@dfinity/agent";
import {
  estimateContractGasLimit,
  getExpectedNonce,
  submitIcTx,
} from "./wrapper-client";
import { encodeApproveCall } from "../erc20";
import { callerEvmAddressFromPrincipalText } from "../principal";
import { getUnwrapRequirements } from "./wrap-client";

export function resolveUnwrapBurnSpenderEvmAddress(factoryAddressHex: string): Uint8Array {
  return factoryAddressHex
    .replace(/^0x/, "")
    .match(/.{1,2}/g)
    ?.reduce<Uint8Array>((acc, value, index) => {
      const next = new Uint8Array(acc);
      next[index] = Number.parseInt(value, 16);
      return next;
    }, new Uint8Array(20)) ?? new Uint8Array(20);
}

export async function approveWrappedTokenIfNeeded(args: {
  assetId: string;
  amount: bigint;
  principalText: string;
  identity: Identity;
}): Promise<void> {
  const ownerEvmAddress = callerEvmAddressFromPrincipalText(args.principalText);
  const requirements = await getUnwrapRequirements({
    assetId: args.assetId,
    amountE8s: args.amount,
    callerEvmAddress: ownerEvmAddress,
  });
  if (requirements.wrappedTokenAddress === null) {
    throw new Error("unwrap.token_not_deployed");
  }
  if (!requirements.approveRequired) {
    return;
  }
  const nonce = await getExpectedNonce(ownerEvmAddress);
  const data = encodeApproveCall(requirements.factoryAddress, args.amount);
  const gasLimit = await estimateContractGasLimit({
    to: requirements.wrappedTokenAddress,
    from: ownerEvmAddress,
    nonce,
    data,
  });
  await submitIcTx({
    to: requirements.wrappedTokenAddress,
    data,
    nonce,
    gasLimit,
    identity: args.identity,
  });
}
