// どこで: routeテスト補助 / 何を: route外から依存差し替えを管理 / なぜ: Next.js route export制約を守りつつAPIテスト可能にするため

import type { HealthDeps, StatusDeps, SubmitDeps, WithdrawDeps } from "./server";

let submitDepsOverride: SubmitDeps | null = null;
let statusDepsOverride: StatusDeps | null = null;
let healthDepsOverride: HealthDeps | null = null;
let withdrawDepsOverride: WithdrawDeps | null = null;

export function getSubmitDepsOverride(): SubmitDeps | null {
  return submitDepsOverride;
}

export function setSubmitDepsOverride(deps: SubmitDeps | null): void {
  submitDepsOverride = deps;
}

export function getStatusDepsOverride(): StatusDeps | null {
  return statusDepsOverride;
}

export function setStatusDepsOverride(deps: StatusDeps | null): void {
  statusDepsOverride = deps;
}

export function getHealthDepsOverride(): HealthDeps | null {
  return healthDepsOverride;
}

export function setHealthDepsOverride(deps: HealthDeps | null): void {
  healthDepsOverride = deps;
}

export function getWithdrawDepsOverride(): WithdrawDeps | null {
  return withdrawDepsOverride;
}

export function setWithdrawDepsOverride(deps: WithdrawDeps | null): void {
  withdrawDepsOverride = deps;
}
