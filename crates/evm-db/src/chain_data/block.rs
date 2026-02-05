//! どこで: Phase1のブロックモデル / 何を: BlockDataとHead / なぜ: 決定的なブロック保存のため

use crate::chain_data::codec::{encode_guarded, mark_decode_failure};
use crate::chain_data::constants::{
    HASH_LEN, HASH_LEN_U32, MAX_BLOCK_DATA_SIZE_U32, MAX_TXS_PER_BLOCK,
};
use crate::chain_data::tx::TxId;
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlockData {
    pub number: u64,
    pub parent_hash: [u8; HASH_LEN],
    pub block_hash: [u8; HASH_LEN],
    pub timestamp: u64,
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
        tx_ids: Vec<TxId>,
        tx_list_hash: [u8; HASH_LEN],
        state_root: [u8; HASH_LEN],
    ) -> Self {
        Self {
            number,
            parent_hash,
            block_hash,
            timestamp,
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
        out.extend_from_slice(&self.tx_list_hash);
        out.extend_from_slice(&self.state_root);
        let len = len_to_u32(self.tx_ids.len(), "block: tx_ids overflow");
        out.extend_from_slice(&len.to_be_bytes());
        for tx_id in self.tx_ids.iter() {
            out.extend_from_slice(&tx_id.0);
        }
        encode_guarded(b"block_data_encode", out, MAX_BLOCK_DATA_SIZE_U32)
    }

    fn into_bytes(self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&self.number.to_be_bytes());
        out.extend_from_slice(&self.parent_hash);
        out.extend_from_slice(&self.block_hash);
        out.extend_from_slice(&self.timestamp.to_be_bytes());
        out.extend_from_slice(&self.tx_list_hash);
        out.extend_from_slice(&self.state_root);
        let len = len_to_u32(self.tx_ids.len(), "block: tx_ids overflow");
        out.extend_from_slice(&len.to_be_bytes());
        for tx_id in self.tx_ids.iter() {
            out.extend_from_slice(&tx_id.0);
        }
        out
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        let base_len = 8 + HASH_LEN + HASH_LEN + 8 + HASH_LEN + HASH_LEN + 4;
        if data.len() < base_len {
            mark_decode_failure(b"block_data", true);
            return BlockData {
                number: 0,
                parent_hash: [0u8; HASH_LEN],
                block_hash: [0u8; HASH_LEN],
                timestamp: 0,
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

impl Storable for Head {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut out = [0u8; 8 + HASH_LEN + 8];
        out[0..8].copy_from_slice(&self.number.to_be_bytes());
        out[8..8 + HASH_LEN].copy_from_slice(&self.block_hash);
        out[8 + HASH_LEN..8 + HASH_LEN + 8].copy_from_slice(&self.timestamp.to_be_bytes());
        encode_guarded(b"head_encode", out.to_vec(), 8 + HASH_LEN_U32 + 8)
    }

    fn into_bytes(self) -> Vec<u8> {
        let mut out = [0u8; 8 + HASH_LEN + 8];
        out[0..8].copy_from_slice(&self.number.to_be_bytes());
        out[8..8 + HASH_LEN].copy_from_slice(&self.block_hash);
        out[8 + HASH_LEN..8 + HASH_LEN + 8].copy_from_slice(&self.timestamp.to_be_bytes());
        out.to_vec()
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
        let mut num = [0u8; 8];
        num.copy_from_slice(&data[0..8]);
        let mut hash = [0u8; HASH_LEN];
        hash.copy_from_slice(&data[8..8 + HASH_LEN]);
        let mut ts = [0u8; 8];
        ts.copy_from_slice(&data[8 + HASH_LEN..8 + HASH_LEN + 8]);
        Self {
            number: u64::from_be_bytes(num),
            block_hash: hash,
            timestamp: u64::from_be_bytes(ts),
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: 8 + HASH_LEN_U32 + 8,
        is_fixed_size: true,
    };
}

fn len_to_u32(len: usize, msg: &str) -> u32 {
    u32::try_from(len).unwrap_or_else(|_| ic_cdk::trap(msg))
}
