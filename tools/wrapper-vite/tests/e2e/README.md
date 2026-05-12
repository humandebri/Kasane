<!-- どこで: wrapper-vite E2E補足 / 何を: 自動化対象と手動確認対象を切り分ける / なぜ: wallet接続付きの実送信確認は手動スモークに残すため -->

# wrapper-vite E2E

- 自動化対象
  - console 初期表示
  - wallet modal の connector 表示
  - `/requests/:requestId` での status modal 再表示
- 手動スモーク対象
  - Oisy 接続
  - MetaMask 接続
  - 実際の request 送信
