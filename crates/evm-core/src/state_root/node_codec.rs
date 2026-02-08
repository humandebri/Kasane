//! どこで: trieノード補助 / 何を: root変換共通化 / なぜ: 更新経路の重複を防ぐため

use evm_db::chain_data::HashKey;

pub fn root_hash_key(root: [u8; 32]) -> Option<HashKey> {
    if root == [0u8; 32] {
        None
    } else {
        Some(HashKey(root))
    }
}
