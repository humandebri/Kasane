// どこで: indexer commit補助 / 何を: token_transfers保存前ガードを提供 / なぜ: guardロジックを単体テストしやすくするため

export function isTokenTransferAmountSupported(amount: bigint): boolean {
  if (amount < 0n) {
    return false;
  }
  // PostgreSQL numeric(78,0) に収まる桁だけを許可する。
  return amount.toString().length <= 78;
}

export const workerCommitGuardTestHooks = {
  isTokenTransferAmountSupported,
};
