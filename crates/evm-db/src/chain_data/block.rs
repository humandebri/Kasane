//! どこで: Phase1のブロックモデル / 何を: BlockDataとHead / なぜ: 決定的なブロック保存のため

use crate::chain_data::codec::{encode_guarded, mark_decode_failure};
use crate::chain_data::constants::{
    BLOCK_BENEFICIARY_LEN, HASH_LEN, HASH_LEN_U32, MAX_BLOCK_DATA_SIZE_U32, MAX_TXS_PER_BLOCK,
};
use crate::chain_data::tx::TxId;
use crate::corrupt_log::record_corrupt;
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;
use zerocopy::byteorder::big_endian::U64;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlockData {
    pub number: u64,
    pub parent_hash: [u8; HASH_LEN],
    pub block_hash: [u8; HASH_LEN],
    pub timestamp: u64,
    pub base_fee_per_gas: u64,
    pub block_gas_limit: u64,
    pub gas_used: u64,
    pub beneficiary: [u8; BLOCK_BENEFICIARY_LEN],
    pub tx_ids: Vec<TxId>,
    pub tx_list_hash: [u8; HASH_LEN],
    pub state_root: [u8; HASH_LEN],
}

impl BlockData {
    pub fn new(
        number: u64,
        parent_hash: [u8; HASH_LEN],
        block_hash: [u8; HASH_LEN],
        timestamp: u64,
        base_fee_per_gas: u64,
        block_gas_limit: u64,
        gas_used: u64,
        beneficiary: [u8; BLOCK_BENEFICIARY_LEN],
        tx_ids: Vec<TxId>,
        tx_list_hash: [u8; HASH_LEN],
        state_root: [u8; HASH_LEN],
    ) -> Self {
        Self {
            number,
            parent_hash,
            block_hash,
            timestamp,
            base_fee_per_gas,
            block_gas_limit,
            gas_used,
            beneficiary,
            tx_ids,
            tx_list_hash,
            state_root,
        }
    }
}

impl Storable for BlockData {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut out = Vec::new();
        out.extend_from_slice(&self.number.to_be_bytes());
        out.extend_from_slice(&self.parent_hash);
        out.extend_from_slice(&self.block_hash);
        out.extend_from_slice(&self.timestamp.to_be_bytes());
        out.extend_from_slice(&self.base_fee_per_gas.to_be_bytes());
        out.extend_from_slice(&self.block_gas_limit.to_be_bytes());
        out.extend_from_slice(&self.gas_used.to_be_bytes());
        out.extend_from_slice(&self.beneficiary);
        out.extend_from_slice(&self.tx_list_hash);
        out.extend_from_slice(&self.state_root);
        let len = match len_to_u32(self.tx_ids.len()) {
            Some(value) => value,
            None => return encode_fallback_block(),
        };
        out.extend_from_slice(&len.to_be_bytes());
        for tx_id in self.tx_ids.iter() {
            out.extend_from_slice(&tx_id.0);
        }
        match encode_guarded(
            b"block_data_encode",
            Cow::Owned(out),
            MAX_BLOCK_DATA_SIZE_U32,
        ) {
            Ok(value) => value,
            Err(_) => encode_fallback_block(),
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&self.number.to_be_bytes());
        out.extend_from_slice(&self.parent_hash);
        out.extend_from_slice(&self.block_hash);
        out.extend_from_slice(&self.timestamp.to_be_bytes());
        out.extend_from_slice(&self.base_fee_per_gas.to_be_bytes());
        out.extend_from_slice(&self.block_gas_limit.to_be_bytes());
        out.extend_from_slice(&self.gas_used.to_be_bytes());
        out.extend_from_slice(&self.beneficiary);
        out.extend_from_slice(&self.tx_list_hash);
        out.extend_from_slice(&self.state_root);
        let len = match len_to_u32(self.tx_ids.len()) {
            Some(value) => value,
            None => return encode_fallback_block().into_owned(),
        };
        out.extend_from_slice(&len.to_be_bytes());
        for tx_id in self.tx_ids.iter() {
            out.extend_from_slice(&tx_id.0);
        }
        out
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        let base_len =
            8 + HASH_LEN + HASH_LEN + 8 + 8 + 8 + 8 + BLOCK_BENEFICIARY_LEN + HASH_LEN + HASH_LEN + 4;
        if data.len() < base_len {
            mark_decode_failure(b"block_data", true);
            return BlockData {
                number: 0,
                parent_hash: [0u8; HASH_LEN],
                block_hash: [0u8; HASH_LEN],
                timestamp: 0,
                base_fee_per_gas: 0,
                block_gas_limit: 0,
                gas_used: 0,
                beneficiary: [0u8; BLOCK_BENEFICIARY_LEN],
                tx_ids: Vec::new(),
                tx_list_hash: [0u8; HASH_LEN],
                state_root: [0u8; HASH_LEN],
            };
        }
        let mut offset = 0;
        let mut num = [0u8; 8];
        num.copy_from_slice(&data[offset..offset + 8]);
        offset += 8;
        let mut parent = [0u8; HASH_LEN];
        parent.copy_from_slice(&data[offset..offset + HASH_LEN]);
        offset += HASH_LEN;
        let mut block = [0u8; HASH_LEN];
        block.copy_from_slice(&data[offset..offset + HASH_LEN]);
        offset += HASH_LEN;
        let mut ts = [0u8; 8];
        ts.copy_from_slice(&data[offset..offset + 8]);
        offset += 8;
        let mut base_fee = [0u8; 8];
        base_fee.copy_from_slice(&data[offset..offset + 8]);
        offset += 8;
        let mut block_gas_limit = [0u8; 8];
        block_gas_limit.copy_from_slice(&data[offset..offset + 8]);
        offset += 8;
        let mut gas_used = [0u8; 8];
        gas_used.copy_from_slice(&data[offset..offset + 8]);
        offset += 8;
        let mut beneficiary = [0u8; BLOCK_BENEFICIARY_LEN];
        beneficiary.copy_from_slice(&data[offset..offset + BLOCK_BENEFICIARY_LEN]);
        offset += BLOCK_BENEFICIARY_LEN;
        let mut tx_list_hash = [0u8; HASH_LEN];
        tx_list_hash.copy_from_slice(&data[offset..offset + HASH_LEN]);
        offset += HASH_LEN;
        let mut state_root = [0u8; HASH_LEN];
        state_root.copy_from_slice(&data[offset..offset + HASH_LEN]);
        offset += HASH_LEN;
        let mut len_bytes = [0u8; 4];
        len_bytes.copy_from_slice(&data[offset..offset + 4]);
        offset += 4;
        let tx_len = match usize::try_from(u32::from_be_bytes(len_bytes)) {
            Ok(value) => value,
            Err(_) => {
                mark_decode_failure(b"block_data", true);
                return BlockData {
                    number: 0,
                    parent_hash: [0u8; HASH_LEN],
                    block_hash: [0u8; HASH_LEN],
                    timestamp: 0,
                    base_fee_per_gas: 0,
                    block_gas_limit: 0,
                    gas_used: 0,
                    beneficiary: [0u8; BLOCK_BENEFICIARY_LEN],
                    tx_ids: Vec::new(),
                    tx_list_hash: [0u8; HASH_LEN],
                    state_root: [0u8; HASH_LEN],
                };
            }
        };
        if tx_len > MAX_TXS_PER_BLOCK {
            mark_decode_failure(b"block_data", true);
            return BlockData {
                number: 0,
                parent_hash: [0u8; HASH_LEN],
                block_hash: [0u8; HASH_LEN],
                timestamp: 0,
                base_fee_per_gas: 0,
                block_gas_limit: 0,
                gas_used: 0,
                beneficiary: [0u8; BLOCK_BENEFICIARY_LEN],
                tx_ids: Vec::new(),
                tx_list_hash: [0u8; HASH_LEN],
                state_root: [0u8; HASH_LEN],
            };
        }
        let expected = base_len + tx_len * HASH_LEN;
        if expected != data.len() {
            mark_decode_failure(b"block_data", true);
            return BlockData {
                number: 0,
                parent_hash: [0u8; HASH_LEN],
                block_hash: [0u8; HASH_LEN],
                timestamp: 0,
                base_fee_per_gas: 0,
                block_gas_limit: 0,
                gas_used: 0,
                beneficiary: [0u8; BLOCK_BENEFICIARY_LEN],
                tx_ids: Vec::new(),
                tx_list_hash: [0u8; HASH_LEN],
                state_root: [0u8; HASH_LEN],
            };
        }
        let mut tx_ids = Vec::with_capacity(tx_len);
        for _ in 0..tx_len {
            let mut tx_id = [0u8; HASH_LEN];
            tx_id.copy_from_slice(&data[offset..offset + HASH_LEN]);
            offset += HASH_LEN;
            tx_ids.push(TxId(tx_id));
        }
        Self {
            number: u64::from_be_bytes(num),
            parent_hash: parent,
            block_hash: block,
            timestamp: u64::from_be_bytes(ts),
            base_fee_per_gas: u64::from_be_bytes(base_fee),
            block_gas_limit: u64::from_be_bytes(block_gas_limit),
            gas_used: u64::from_be_bytes(gas_used),
            beneficiary,
            tx_ids,
            tx_list_hash,
            state_root,
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: MAX_BLOCK_DATA_SIZE_U32,
        is_fixed_size: false,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Head {
    pub number: u64,
    pub block_hash: [u8; HASH_LEN],
    pub timestamp: u64,
}

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned,
)]
#[repr(C)]
struct HeadWire {
    number: U64,
    block_hash: [u8; HASH_LEN],
    timestamp: U64,
}

impl HeadWire {
    fn new(head: &Head) -> Self {
        Self {
            number: U64::new(head.number),
            block_hash: head.block_hash,
            timestamp: U64::new(head.timestamp),
        }
    }
}

impl Storable for Head {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let wire = HeadWire::new(self);
        match encode_guarded(
            b"head_encode",
            Cow::Owned(wire.as_bytes().to_vec()),
            8 + HASH_LEN_U32 + 8,
        ) {
            Ok(value) => value,
            Err(_) => Cow::Owned(vec![0u8; (8 + HASH_LEN_U32 + 8) as usize]),
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        let wire = HeadWire::new(&self);
        wire.as_bytes().to_vec()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != 8 + HASH_LEN + 8 {
            mark_decode_failure(b"head", true);
            return Head {
                number: 0,
                block_hash: [0u8; HASH_LEN],
                timestamp: 0,
            };
        }
        let wire = match HeadWire::read_from_bytes(data) {
            Ok(value) => value,
            Err(_) => {
                mark_decode_failure(b"head", true);
                return Head {
                    number: 0,
                    block_hash: [0u8; HASH_LEN],
                    timestamp: 0,
                };
            }
        };
        Self {
            number: wire.number.get(),
            block_hash: wire.block_hash,
            timestamp: wire.timestamp.get(),
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: 8 + HASH_LEN_U32 + 8,
        is_fixed_size: true,
    };
}

fn len_to_u32(len: usize) -> Option<u32> {
    match u32::try_from(len) {
        Ok(value) => Some(value),
        Err(_) => {
            record_corrupt(b"block_len");
            None
        }
    }
}

fn encode_fallback_block() -> Cow<'static, [u8]> {
    let mut out =
        Vec::with_capacity(
            8 + HASH_LEN + HASH_LEN + 8 + 8 + 8 + 8 + BLOCK_BENEFICIARY_LEN + HASH_LEN + HASH_LEN
                + 4,
        );
    out.extend_from_slice(&0u64.to_be_bytes());
    out.extend_from_slice(&[0u8; HASH_LEN]);
    out.extend_from_slice(&[0u8; HASH_LEN]);
    out.extend_from_slice(&0u64.to_be_bytes());
    out.extend_from_slice(&0u64.to_be_bytes());
    out.extend_from_slice(&0u64.to_be_bytes());
    out.extend_from_slice(&0u64.to_be_bytes());
    out.extend_from_slice(&[0u8; BLOCK_BENEFICIARY_LEN]);
    out.extend_from_slice(&[0u8; HASH_LEN]);
    out.extend_from_slice(&[0u8; HASH_LEN]);
    out.extend_from_slice(&0u32.to_be_bytes());
    match encode_guarded(
        b"block_data_encode",
        Cow::Owned(out),
        MAX_BLOCK_DATA_SIZE_U32,
    ) {
        Ok(value) => value,
        Err(_) => Cow::Owned(vec![0u8; MAX_BLOCK_DATA_SIZE_U32 as usize]),
    }
}
