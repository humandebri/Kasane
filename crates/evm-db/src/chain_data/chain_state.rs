//! どこで: chain_data のヘッダ状態 / 何を: 固定サイズのStableStateV1 / なぜ: upgrade耐性と最小メタ保持のため

use crate::chain_data::codec::{encode_guarded, mark_decode_failure};
use crate::chain_data::constants::{CHAIN_ID, CHAIN_STATE_SIZE_U32};
use crate::chain_data::runtime_defaults::{
    DEFAULT_BASE_FEE, DEFAULT_MIN_GAS_PRICE, DEFAULT_MINING_INTERVAL_MS, DEFAULT_MIN_PRIORITY_FEE,
};
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;
use zerocopy::byteorder::big_endian::{U32, U64};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChainStateV1 {
    pub schema_version: u32,
    pub chain_id: u64,
    pub last_block_number: u64,
    pub last_block_time: u64,
    pub auto_mine_enabled: bool,
    pub is_producing: bool,
    pub mining_scheduled: bool,
    pub next_queue_seq: u64,
    pub mining_interval_ms: u64,
    pub base_fee: u64,
    pub min_gas_price: u64,
    pub min_priority_fee: u64,
}

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned,
)]
#[repr(C)]
struct ChainStateWire {
    schema_version: U32,
    chain_id: U64,
    last_block_number: U64,
    last_block_time: U64,
    flags: u8,
    _pad0: [u8; 3],
    next_queue_seq: U64,
    mining_interval_ms: U64,
    base_fee: U64,
    min_gas_price: U64,
    min_priority_fee: U64,
}

impl ChainStateWire {
    fn new(state: &ChainStateV1) -> Self {
        Self {
            schema_version: U32::new(state.schema_version),
            chain_id: U64::new(state.chain_id),
            last_block_number: U64::new(state.last_block_number),
            last_block_time: U64::new(state.last_block_time),
            flags: state.flags(),
            _pad0: [0u8; 3],
            next_queue_seq: U64::new(state.next_queue_seq),
            mining_interval_ms: U64::new(state.mining_interval_ms),
            base_fee: U64::new(state.base_fee),
            min_gas_price: U64::new(state.min_gas_price),
            min_priority_fee: U64::new(state.min_priority_fee),
        }
    }
}

impl ChainStateV1 {
    pub fn new(chain_id: u64) -> Self {
        Self {
            schema_version: 2,
            chain_id,
            last_block_number: 0,
            last_block_time: 0,
            auto_mine_enabled: false,
            is_producing: false,
            mining_scheduled: false,
            next_queue_seq: 0,
            mining_interval_ms: DEFAULT_MINING_INTERVAL_MS,
            base_fee: DEFAULT_BASE_FEE,
            min_gas_price: DEFAULT_MIN_GAS_PRICE,
            min_priority_fee: DEFAULT_MIN_PRIORITY_FEE,
        }
    }

    fn flags(&self) -> u8 {
        let mut out = 0u8;
        if self.auto_mine_enabled {
            out |= 1 << 0;
        }
        if self.is_producing {
            out |= 1 << 1;
        }
        if self.mining_scheduled {
            out |= 1 << 2;
        }
        out
    }

    fn apply_flags(&mut self, flags: u8) {
        self.auto_mine_enabled = (flags & (1 << 0)) != 0;
        self.is_producing = (flags & (1 << 1)) != 0;
        self.mining_scheduled = (flags & (1 << 2)) != 0;
    }
}

impl Storable for ChainStateV1 {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let wire = ChainStateWire::new(self);
        match encode_guarded(
            b"chain_state",
            Cow::Owned(wire.as_bytes().to_vec()),
            CHAIN_STATE_SIZE_U32,
        ) {
            Ok(value) => value,
            Err(_) => Cow::Owned(vec![0u8; CHAIN_STATE_SIZE_U32 as usize]),
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != 72 {
            mark_decode_failure(b"chain_state", false);
            return ChainStateV1::new(CHAIN_ID);
        }
        let wire = match ChainStateWire::read_from_bytes(data) {
            Ok(value) => value,
            Err(_) => {
                mark_decode_failure(b"chain_state", false);
                return ChainStateV1::new(CHAIN_ID);
            }
        };
        let mut state = Self {
            schema_version: wire.schema_version.get(),
            chain_id: wire.chain_id.get(),
            last_block_number: wire.last_block_number.get(),
            last_block_time: wire.last_block_time.get(),
            auto_mine_enabled: false,
            is_producing: false,
            mining_scheduled: false,
            next_queue_seq: wire.next_queue_seq.get(),
            mining_interval_ms: wire.mining_interval_ms.get(),
            base_fee: wire.base_fee.get(),
            min_gas_price: wire.min_gas_price.get(),
            min_priority_fee: wire.min_priority_fee.get(),
        };
        state.apply_flags(wire.flags);
        state
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: CHAIN_STATE_SIZE_U32,
        is_fixed_size: true,
    };
}
