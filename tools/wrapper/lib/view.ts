// どこで: 表示用変換 / 何を: 状態に応じたラベル・バッジ色を定義 / なぜ: dispatchとexecutionの意味をUIで明確に分離するため

import type { DispatchStatus, ExecutionStatus } from "./types";

export function dispatchBadgeVariant(status: DispatchStatus | null): "neutral" | "info" | "success" | "danger" {
  if (status === "Dispatching") {
    return "info";
  }
  if (status === "Dispatched") {
    return "success";
  }
  if (status === "DispatchFailed") {
    return "danger";
  }
  return "neutral";
}

export function executionBadgeVariant(status: ExecutionStatus | null): "neutral" | "info" | "success" | "danger" {
  if (status === "Running") {
    return "info";
  }
  if (status === "Succeeded") {
    return "success";
  }
  if (status === "Failed") {
    return "danger";
  }
  return "neutral";
}
