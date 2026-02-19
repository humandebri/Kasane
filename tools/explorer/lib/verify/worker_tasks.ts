// どこで: verify worker補助層 / 何を: 定期実行判定と非同期安全実行を提供 / なぜ: 本体を簡潔にしテストを独立させるため

export function shouldRunPeriodicTask(nowMs: number, lastRunMs: number, intervalMs: number): boolean {
  return nowMs - lastRunMs >= intervalMs;
}

export function runBackgroundTask(label: string, task: () => Promise<void>): void {
  void task().catch((err: unknown) => {
    console.error(`[verify-worker] ${label} failed`, err);
  });
}
