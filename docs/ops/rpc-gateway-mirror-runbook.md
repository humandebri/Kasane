# RPC Gateway Mirror Runbook

## 目的
- monorepo を正本として維持しながら、`tools/rpc-gateway` を `kasane-network/rpc-gateway` へミラー公開する。
- 別repoは配布・参照用とし、編集起点にしない。

## 前提
- 作業ディレクトリは monorepo root
- `gh` で GitHub 認証済み
- ミラー先 repo: `kasane-network/rpc-gateway`

## 初回セットアップ
1. ミラー先 repo を作成
```bash
gh repo create kasane-network/rpc-gateway --public
```

2. subtree 用ブランチを作る
```bash
git subtree split --prefix=tools/rpc-gateway -b gateway-split
```

3. 初回 push
```bash
git push https://github.com/kasane-network/rpc-gateway.git gateway-split:main --force
```

4. 後片付け（任意）
```bash
git branch -D gateway-split
```

## 通常更新手順
1. monorepo 側で必要変更を main にマージ
2. 最新 main で subtree split
```bash
git checkout main
git pull
git subtree split --prefix=tools/rpc-gateway -b gateway-split
```

3. ミラー先へ push
```bash
git push https://github.com/kasane-network/rpc-gateway.git gateway-split:main --force
```

4. 後片付け（任意）
```bash
git branch -D gateway-split
```

## 失敗時リカバリ
- push 拒否/履歴衝突時:
  - ミラー先 `main` は mirror 管理前提のため `--force` を使う
- `gh` 認証切れ:
```bash
gh auth login -h github.com
```
- 期待差分が出ない:
  - `tools/rpc-gateway` 配下に変更が入っているか確認

## タグ運用（任意）
- ミラー先で配布タグを打つ場合は、subtree push 後にミラー先で実施する。
- 正本の履歴管理は monorepo 側で行う。

## 影響範囲（別repo利用者向け）
次の変更はミラー利用者の挙動/理解に直接影響する。

- `tools/rpc-gateway/README.md`
- `tools/rpc-gateway/ops/*`
- `tools/rpc-gateway/contracts/*`
- `tools/rpc-gateway/src/*`（実装）

## 互換性ガード
mirror 更新前に最低限次を通す。

```bash
scripts/check_did_sync.sh
scripts/check_gateway_api_compat_baseline.sh
scripts/check_gateway_matrix_sync.sh
cd tools/rpc-gateway && npm test && npm run build
```
