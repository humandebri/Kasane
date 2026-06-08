# ic-evm-stack repo運用計画

## 方針

- `ic-evm-stack` を公開OSS本流にする。
- 本repo `Kasane` はKasane testnet/mainnetの運用repoとして残す。
- 本repoにはKasane固有のcanister id、RPC URL、Explorer URL、Contabo/systemd、mainnet runbook、運用reportsを保持する。
- 公開repoには再利用可能なcanister、RPC gateway、indexer、explorer、quickstart、汎用smokeだけを置く。

## 初回構成

公開repo作成先:

```text
/Users/0xhude/Desktop/ICP/ic-evm-stack
```

本repo側の将来配置:

```text
external/ic-evm-stack
deployments/kasane-testnet
deployments/kasane-mainnet
```

`external/ic-evm-stack` は公開repoのrelease tagをsubmoduleで固定する。初回はsubmodule化せず、公開repoのtag作成後に追加する。

## 更新フロー

1. `ic-evm-stack` で実装する。
2. CIとlocal smokeを通す。
3. release tagを作る。
4. 本repoの `external/ic-evm-stack` をtagへ更新する。
5. `deployments/kasane-*` の設定でpreflight/smokeを実行する。
6. Kasane環境へdeployする。

## 禁止

- private運用情報を公開repoへコピーしない。
- `docs/ops/reports/*` を公開repoへ入れない。
- `.env`, `.env.local`, PEM、HMAC secret、deploy tokenを公開repoへ入れない。
- `tools/wrapper-vite` は初回公開MVPへ含めない。
