// どこで: native ICP withdraw client flow / 何を: quote後のEVM送信入力を構築 / なぜ: fee以下のtxをprecompileへ送らないため

import { quoteNativeWithdrawal } from "./canister/wrap-client";
import {
  encodeNativeWithdrawPayload,
  NATIVE_WITHDRAW_PRECOMPILE_ADDRESS,
} from "./request-id";
import { bytesToHex } from "./utils";

export type NativeWithdrawQuote = {
  nativeLedgerCanister: string;
  ledgerFeeE8s: bigint;
  receiveAmountE8s: bigint;
};

export type NativeWithdrawTransaction = {
  to: string;
  data: string;
  valueWei: bigint;
  ledgerFeeE8s: bigint;
  receiveAmountE8s: bigint;
};

const WEI_PER_E8S = 10_000_000_000n;

export async function prepareNativeWithdrawTransaction(args: {
  amountE8s: bigint;
  recipient: string;
  readQuote?: (args: { amountE8s: bigint; recipient: string }) => Promise<NativeWithdrawQuote>;
}): Promise<NativeWithdrawTransaction> {
  const readQuote = args.readQuote ?? quoteNativeWithdrawal;
  const quote = await readQuote({
    amountE8s: args.amountE8s,
    recipient: args.recipient,
  });
  if (args.amountE8s <= quote.ledgerFeeE8s) {
    throw new Error("native_withdraw.amount_not_above_fee");
  }
  return {
    to: bytesToHex(NATIVE_WITHDRAW_PRECOMPILE_ADDRESS),
    data: bytesToHex(encodeNativeWithdrawPayload({ recipient: args.recipient })),
    valueWei: args.amountE8s * WEI_PER_E8S,
    ledgerFeeE8s: quote.ledgerFeeE8s,
    receiveAmountE8s: quote.receiveAmountE8s,
  };
}
