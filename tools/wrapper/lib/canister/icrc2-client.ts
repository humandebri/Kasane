// どこで: ICRC-2 クライアント / 何を: ウォレット署名で icrc2_approve を実行 / なぜ: approve 成功後に wrap submit を進めるため

import { Actor, type ActorSubclass, type Identity } from "@dfinity/agent";
import { IDL } from "@dfinity/candid";
import { Principal } from "@dfinity/principal";
import { getIdentityAgent, getQueryAgent } from "./agent";

type ApproveError =
  | { BadFee: { expected_fee: bigint } }
  | { InsufficientFunds: { balance: bigint } }
  | { AllowanceChanged: { current_allowance: bigint } }
  | { Expired: { ledger_time: bigint } }
  | { TooOld: null }
  | { CreatedInFuture: { ledger_time: bigint } }
  | { Duplicate: { duplicate_of: bigint } }
  | { TemporarilyUnavailable: null }
  | { GenericError: { error_code: bigint; message: string } };

type ApproveResult = { Ok: bigint } | { Err: ApproveError };
type AllowanceResult = {
  allowance: bigint;
  expires_at: [] | [bigint];
};

type Icrc2Actor = ActorSubclass<{
  icrc2_approve: (args: {
    from_subaccount: [] | [Uint8Array];
    spender: { owner: Principal; subaccount: [] | [Uint8Array] };
    amount: bigint;
    expected_allowance: [] | [bigint];
    expires_at: [] | [bigint];
    fee: [] | [bigint];
    memo: [] | [Uint8Array];
    created_at_time: [] | [bigint];
  }) => Promise<ApproveResult>;
  icrc2_allowance: (args: {
    account: { owner: Principal; subaccount: [] | [Uint8Array] };
    spender: { owner: Principal; subaccount: [] | [Uint8Array] };
  }) => Promise<AllowanceResult>;
}>;

const icrc2IdlFactory: IDL.InterfaceFactory = ({ IDL: I }) => {
  const Account = I.Record({
    owner: I.Principal,
    subaccount: I.Opt(I.Vec(I.Nat8)),
  });
  const ApproveArgs = I.Record({
    from_subaccount: I.Opt(I.Vec(I.Nat8)),
    spender: Account,
    amount: I.Nat,
    expected_allowance: I.Opt(I.Nat),
    expires_at: I.Opt(I.Nat64),
    fee: I.Opt(I.Nat),
    memo: I.Opt(I.Vec(I.Nat8)),
    created_at_time: I.Opt(I.Nat64),
  });
  const ApproveError = I.Variant({
    BadFee: I.Record({ expected_fee: I.Nat }),
    InsufficientFunds: I.Record({ balance: I.Nat }),
    AllowanceChanged: I.Record({ current_allowance: I.Nat }),
    Expired: I.Record({ ledger_time: I.Nat64 }),
    TooOld: I.Null,
    CreatedInFuture: I.Record({ ledger_time: I.Nat64 }),
    Duplicate: I.Record({ duplicate_of: I.Nat }),
    TemporarilyUnavailable: I.Null,
    GenericError: I.Record({ error_code: I.Nat, message: I.Text }),
  });
  return I.Service({
    icrc2_approve: I.Func([ApproveArgs], [I.Variant({ Ok: I.Nat, Err: ApproveError })], []),
    icrc2_allowance: I.Func([I.Record({ account: Account, spender: Account })], [I.Record({
      allowance: I.Nat,
      expires_at: I.Opt(I.Nat64),
    })], ["query"]),
  });
};

function decodeApproveError(err: ApproveError): string {
  if ("BadFee" in err) {
    return `icrc2.approve.bad_fee:${err.BadFee.expected_fee.toString()}`;
  }
  if ("InsufficientFunds" in err) {
    return `icrc2.approve.insufficient_funds:${err.InsufficientFunds.balance.toString()}`;
  }
  if ("AllowanceChanged" in err) {
    return `icrc2.approve.allowance_changed:${err.AllowanceChanged.current_allowance.toString()}`;
  }
  if ("Expired" in err) {
    return `icrc2.approve.expired:${err.Expired.ledger_time.toString()}`;
  }
  if ("TooOld" in err) {
    return "icrc2.approve.too_old";
  }
  if ("CreatedInFuture" in err) {
    return `icrc2.approve.created_in_future:${err.CreatedInFuture.ledger_time.toString()}`;
  }
  if ("Duplicate" in err) {
    return `icrc2.approve.duplicate:${err.Duplicate.duplicate_of.toString()}`;
  }
  if ("TemporarilyUnavailable" in err) {
    return "icrc2.approve.temporarily_unavailable";
  }
  return `icrc2.approve.generic:${err.GenericError.message}`;
}

export async function approveLedgerSpend(args: {
  ledgerCanisterId: string;
  spenderCanisterId: string;
  amount: bigint;
  identity: Identity;
}): Promise<bigint> {
  const agent = await getIdentityAgent(args.identity);
  const actor = Actor.createActor<Icrc2Actor>(icrc2IdlFactory, {
    canisterId: args.ledgerCanisterId,
    agent,
  });

  const out = await actor.icrc2_approve({
    from_subaccount: [],
    spender: { owner: Principal.fromText(args.spenderCanisterId), subaccount: [] },
    amount: args.amount,
    expected_allowance: [],
    expires_at: [],
    fee: [],
    memo: [],
    created_at_time: [],
  });

  if ("Err" in out) {
    throw new Error(decodeApproveError(out.Err));
  }
  return out.Ok;
}

export async function getLedgerAllowance(args: {
  ledgerCanisterId: string;
  ownerPrincipalText: string;
  spenderCanisterId: string;
}): Promise<bigint> {
  const actor = Actor.createActor<Icrc2Actor>(icrc2IdlFactory, {
    canisterId: args.ledgerCanisterId,
    agent: await getQueryAgent(),
  });
  const out = await actor.icrc2_allowance({
    account: { owner: Principal.fromText(args.ownerPrincipalText), subaccount: [] },
    spender: { owner: Principal.fromText(args.spenderCanisterId), subaccount: [] },
  });
  return out.allowance;
}
