# PR0 Differential Runbook

どこで: PR0差分検証  
何を: ローカル実装と参照実装のスナップショット差分を比較  
なぜ: 後続PRで意図しないセマンティクス差分を早期検知するため

## 1. ローカル実装のスナップショットを生成

```bash
scripts/pr0_capture_local_snapshot.sh /tmp/pr0_snapshot_local.txt
```

出力形式は以下の2行固定です。

- `SNAPSHOT_TX_MATRIX: ...`
- `SNAPSHOT_BLOCK: ...`

## 2. 参照実装スナップショットを用意

`docs/ops/pr0_snapshot_reference.txt` と同じ2行形式で、参照実装（reth等）から生成した値を保存します。

例:

```text
SNAPSHOT_TX_MATRIX: tx_statuses=[...]
SNAPSHOT_BLOCK: number=... block_hash=... tx_list_hash=... state_root=...
```

## 3. 差分比較を実行

```bash
scripts/pr0_differential_compare.sh /tmp/pr0_snapshot_local.txt docs/ops/pr0_snapshot_reference.txt
```

差分がなければ `OK`、差分があれば `NG` で終了します。

## 4. CIで常時チェック

`scripts/ci-local.sh` はデフォルトで次を実行します。

1. ローカルスナップショット生成  
2. `docs/ops/pr0_snapshot_reference.txt` とのdiff比較

参照先を差し替える場合は環境変数を使います。

```bash
PR0_DIFF_LOCAL=/tmp/pr0_snapshot_local.txt \
PR0_DIFF_REFERENCE=/path/to/reference.txt \
scripts/ci-local.sh
```
