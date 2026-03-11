// どこで: wrapper の EVM token/factory client
// 何を: unwrap 前の token 解決・allowance 確認・approve 送信を提供
// なぜ: approve -> unwrap の順を強制して burn 前提の unwrap を成立させるため

import type { Identity } from "@dfinity/agent";
import { loadConfig } from "../config";
import {
  callReadonlyContract,
  estimateContractGasLimit,
  getExpectedNonce,
  submitIcTx,
} from "./wrapper-client";
import {
  decodeAddressReturnData,
  decodeUint256ReturnData,
  encodeAllowanceCall,
  encodeApproveCall,
  encodeFactoryGetTokenAddressCall,
} from "../erc20";
import { callerEvmAddressFromPrincipalText } from "../principal";
import { hexToBytes } from "../utils";

export function resolveUnwrapBurnSpenderEvmAddress(factoryAddressHex: string): Uint8Array {
  return hexToBytes(factoryAddressHex);
}

export async function getWrappedTokenAddress(assetId: string): Promise<Uint8Array | null> {
  const cfg = loadConfig();
  const returnData = await callReadonlyContract({
    to: hexToBytes(cfg.evmWrapFactory),
    data: encodeFactoryGetTokenAddressCall(assetId),
  });
  return decodeAddressReturnData(returnData);
}

export async function getWrappedTokenAllowance(args: {
  tokenAddress: Uint8Array;
  ownerEvmAddress: Uint8Array;
  spenderEvmAddress: Uint8Array;
}): Promise<bigint> {
  const returnData = await callReadonlyContract({
    to: args.tokenAddress,
    data: encodeAllowanceCall(args.ownerEvmAddress, args.spenderEvmAddress),
    from: args.ownerEvmAddress,
  });
  return decodeUint256ReturnData(returnData);
}

export async function approveWrappedTokenIfNeeded(args: {
  assetId: string;
  amount: bigint;
  principalText: string;
  identity: Identity;
}): Promise<void> {
  const cfg = loadConfig();
  const tokenAddress = await getWrappedTokenAddress(args.assetId);
  if (!tokenAddress) {
    throw new Error("unwrap.token_not_deployed");
  }
  const ownerEvmAddress = callerEvmAddressFromPrincipalText(args.principalText);
  const spenderEvmAddress = resolveUnwrapBurnSpenderEvmAddress(cfg.evmWrapFactory);
  const allowance = await getWrappedTokenAllowance({
    tokenAddress,
    ownerEvmAddress,
    spenderEvmAddress,
  });
  if (allowance >= args.amount) {
    return;
  }
  const nonce = await getExpectedNonce(ownerEvmAddress);
  const data = encodeApproveCall(spenderEvmAddress, args.amount);
  const gasLimit = await estimateContractGasLimit({
    to: tokenAddress,
    from: ownerEvmAddress,
    nonce,
    data,
  });
  await submitIcTx({
    to: tokenAddress,
    data,
    nonce,
    gasLimit,
    identity: args.identity,
  });
}
