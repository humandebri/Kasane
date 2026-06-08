# Verus TCB台帳

この文書はVerus証明対象外の仮定を管理する。
証明対象外のRust業務ロジックを追加する場合、理由と検証手段を追記する。

## 証明対象

- `crates/verified-*`: 純粋な状態遷移、上限判定、codec境界、prune計算、state diff適用判定。
- `crates/verified-*/src`: adapterが直接呼ぶ実装関数に付けた `requires` / `ensures` / `invariant` 仕様。

## TCB

| ID | 領域 | 仮定 | 代替検証 |
| --- | --- | --- | --- |
| TCB-revm | `revm` | EVM実行意味論、gas消費、halt理由が上流仕様どおりである | 互換E2E、既存revmテスト、固定feature検査 |
| TCB-alloy | `alloy-*` | RLP、署名、Ethereum型のdecode/encodeが仕様どおりである | 既存unit/integration、RPC互換smoke |
| TCB-keccak | `keccak` | hash実装がEthereum互換である | 既知ベクトルテスト、state rootテスト |
| TCB-state-root | state root/account state | pruning証明は履歴削除の観測整合だけを対象とし、current account state、trie、state root正当性は証明しない | state root migration/unit test、revm DB test、運用smoke |
| TCB-dfinity | DFINITY crates | `ic-cdk`、`ic-stable-structures`、Candid、timerが公開契約どおり動く | PocketIC、upgrade/smoke |
| TCB-ic-runtime | IC runtime | caller、time、cycles、performance counter、stable memoryがIC仕様どおりである | local/mainnet smoke、運用監視 |
| TCB-ic-query-precompile | ICP query precompile 外部境界 | 入口は `composite_query` であり、target は query / composite query method だけを登録する。allowlist は method selection のTCBである。`Call::bounded_wait`、IC routing、remote canisterの応答正当性、`SysUnknown` / timeout、cross-subnet rejectはVerus対象外である。raw Candid bytes は `take_raw_args` / `into_bytes` で中継し、再エンコードしない。ローカル証明対象はallowlist済みquery requestを1回だけ発火し、2-pass snapshot guardとgas制約付きでEVMへ戻す境界までとする。account / storage / code の永続変異は `evm_state_epoch` で検出する。await中に変わると async eth_call 結果へ影響する mutable execution input は `QueryCallSnapshot` に含めるか、該当 `PrecompileAccess` profile で無効化する | allowlist adapter test、`eth_call_object_async` test、PocketIC composite query E2E、PBT、mainnet/local smoke |
| TCB-typescript | TypeScript tools | explorer/indexer/gateway UIはVerus対象外である | TypeScript検査、npm test、E2E |
| TCB-github-actions | GitHub Actions | 固定したVerus release assetとRust toolchain取得が成功する | `scripts/verify-verus.sh` とCIログ |

## 追加ルール

- 新規Rust業務ロジックは `crates/verified-*` へ置く。
- TCBへ逃がす場合、上表へID、仮定、代替検証を追加する。
- adapter層はIC API、stable memory、time、cycles、Candid、revm呼び出しだけを行う。
- fallback/shimで未証明分岐を増やさない。
