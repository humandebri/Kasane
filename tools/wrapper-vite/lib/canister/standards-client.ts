// どこで: canister capability probe
// 何を: ICRC-10 supported standards を読んで Oisy 対応可否を判定
// なぜ: static flag ではなく deploy 済み capability で UI を開閉するため

import type { ActorSubclass } from "@icp-sdk/core/agent";
import { IDL } from "@icp-sdk/core/candid";
import { createQueryActor } from "./actor-utils";

type StandardsActor = ActorSubclass<{
  icrc10_supported_standards: () => Promise<Array<{ name: string; url: string }>>;
}>;

const standardsIdlFactory: IDL.InterfaceFactory = ({ IDL: I }) => I.Service({
  icrc10_supported_standards: I.Func([], [I.Vec(I.Record({
    name: I.Text,
    url: I.Text,
  }))], ["query"]),
});

async function readSupportedStandards(canisterId: string): Promise<string[]> {
  const actor = await createQueryActor<StandardsActor>({
    canisterId,
    idlFactory: standardsIdlFactory,
  });
  const records = await actor.icrc10_supported_standards();
  return records.map((record) => record.name);
}

export async function hasIcrc21Support(canisterId: string | null): Promise<boolean> {
  if (canisterId === null || canisterId.trim() === "") {
    return false;
  }
  try {
    const names = await readSupportedStandards(canisterId.trim());
    return names.includes("ICRC-21");
  } catch {
    return false;
  }
}
