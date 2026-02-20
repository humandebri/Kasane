//! どこで: evm-core共通定数 / 何を: 手数料受取先アドレス定義 / なぜ: 実行ロジックから定数を分離し可読性を上げるため

use revm::primitives::{address, Address};

/// EVM実行時にbeneficiaryとして使う手数料受取先。
pub(crate) const FEE_RECIPIENT: Address = address!("0x6b9b5fd62cc66fc9fef74210c9298b1b6bcbfc52");
