//! どこで: chain_data のTx位置 / 何を: Pending/Included/Dropped の最小表現 / なぜ: pending可視化を安定化するため

use crate::chain_data::codec::{encode_guarded, mark_decode_failure};
use crate::chain_data::constants::TX_LOC_SIZE_U32;
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;
use wincode::{SchemaRead, SchemaWrite};

#[derive(Clone, Copy, Debug, SchemaRead, SchemaWrite, Eq, PartialEq)]
#[repr(u8)]
pub enum TxLocKind {
    Queued = 0,
    Included = 1,
    Dropped = 2,
}

#[derive(Clone, Copy, Debug, SchemaRead, SchemaWrite, Eq, PartialEq)]
pub struct TxLoc {
    pub kind: TxLocKind,
    pub seq: u64,
    pub block_number: u64,
    pub tx_index: u32,
    pub drop_code: u16,
}

impl TxLoc {
    pub fn queued(seq: u64) -> Self {
        Self {
            kind: TxLocKind::Queued,
            seq,
            block_number: 0,
            tx_index: 0,
            drop_code: 0,
        }
    }

    pub fn included(block_number: u64, tx_index: u32) -> Self {
        Self {
            kind: TxLocKind::Included,
            seq: 0,
            block_number,
            tx_index,
            drop_code: 0,
        }
    }

    pub fn dropped(code: u16) -> Self {
        Self {
            kind: TxLocKind::Dropped,
            seq: 0,
            block_number: 0,
            tx_index: 0,
            drop_code: code,
        }
    }
}

impl Storable for TxLoc {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let encoded = wincode::config::serialize(self, tx_loc_wincode_config())
            .unwrap_or_else(|_| ic_cdk::trap("tx_loc: encode failed"));
        encode_guarded(b"tx_loc_encode", encoded, TX_LOC_SIZE_U32)
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        match wincode::config::deserialize::<TxLoc, _>(data, tx_loc_wincode_config()) {
            Ok(value) => value,
            _ => decode_legacy_tx_loc(data),
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: TX_LOC_SIZE_U32,
        is_fixed_size: true,
    };
}

fn tx_loc_wincode_config() -> impl wincode::config::Config {
    wincode::config::Configuration::default()
        .with_big_endian()
        .with_fixint_encoding()
        .with_preallocation_size_limit::<{ TX_LOC_SIZE_U32 as usize }>()
}

// NOTE: 旧形式デコードは移行ウィンドウのための例外経路。
// 通常経路に旧decodeを増やさない方針を維持し、v3安定化後に削除する。
fn decode_legacy_tx_loc(data: &[u8]) -> TxLoc {
    if data.len() != 24 {
        mark_decode_failure(b"tx_loc", true);
        return TxLoc::queued(0);
    }
    let kind = match data[0] {
        0 => TxLocKind::Queued,
        1 => TxLocKind::Included,
        2 => TxLocKind::Dropped,
        _ => {
            mark_decode_failure(b"tx_loc_kind", true);
            TxLocKind::Queued
        }
    };
    let mut seq = [0u8; 8];
    seq.copy_from_slice(&data[1..9]);
    let mut block = [0u8; 8];
    block.copy_from_slice(&data[9..17]);
    let mut idx = [0u8; 4];
    idx.copy_from_slice(&data[17..21]);
    let mut code = [0u8; 2];
    code.copy_from_slice(&data[21..23]);
    TxLoc {
        kind,
        seq: u64::from_be_bytes(seq),
        block_number: u64::from_be_bytes(block),
        tx_index: u32::from_be_bytes(idx),
        drop_code: u16::from_be_bytes(code),
    }
}
