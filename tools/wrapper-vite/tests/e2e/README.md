<!-- どこで: wrapper-vite E2E補足 / 何を: 自動化対象と手動確認対象を切り分ける / なぜ: wallet接続付きの保存確認は現段階では手動スモークに残すため -->

# wrapper-vite E2E

- 自動化対象
  - 未接続時の `Recent Requests` 表示
  - `/requests/:requestId` での status modal 再表示
- 手動スモーク対象
  - wallet 接続
  - 実際の request 送信
  - Juno Datastore 保存と reload 後の再取得
