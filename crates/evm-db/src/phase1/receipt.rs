//! どこで: Phase1のReceipt / 何を: 最小結果の保存 / なぜ: 参照と互換のため

use crate::phase1::constants::{HASH_LEN, RECEIPT_CONTRACT_ADDR_LEN, RECEIPT_SIZE_U32};
use crate::phase1::tx::TxId;
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReceiptLike {
    pub tx_id: TxId,
    pub block_number: u64,
    pub tx_index: u32,
    pub status: u8,
    pub gas_used: u64,
    pub return_data_hash: [u8; HASH_LEN],
    pub contract_address: Option<[u8; RECEIPT_CONTRACT_ADDR_LEN]>,
}

impl Storable for ReceiptLike {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut out = Vec::with_capacity(32 + 8 + 4 + 1 + 8 + 32 + 1 + RECEIPT_CONTRACT_ADDR_LEN);
        out.extend_from_slice(&self.tx_id.0);
        out.extend_from_slice(&self.block_number.to_be_bytes());
        out.extend_from_slice(&self.tx_index.to_be_bytes());
        out.push(self.status);
        out.extend_from_slice(&self.gas_used.to_be_bytes());
        out.extend_from_slice(&self.return_data_hash);
        match self.contract_address {
            Some(addr) => {
                out.push(1);
                out.extend_from_slice(&addr);
            }
            None => {
                out.push(0);
                out.extend_from_slice(&[0u8; RECEIPT_CONTRACT_ADDR_LEN]);
            }
        }
        Cow::Owned(out)
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        let expected = 32 + 8 + 4 + 1 + 8 + 32 + 1 + RECEIPT_CONTRACT_ADDR_LEN;
        if data.len() != expected {
            ic_cdk::trap("receipt: invalid length");
        }
        let mut offset = 0;
        let mut tx_id = [0u8; 32];
        tx_id.copy_from_slice(&data[offset..offset + 32]);
        offset += 32;
        let mut bn = [0u8; 8];
        bn.copy_from_slice(&data[offset..offset + 8]);
        offset += 8;
        let mut ti = [0u8; 4];
        ti.copy_from_slice(&data[offset..offset + 4]);
        offset += 4;
        let status = data[offset];
        offset += 1;
        let mut gas = [0u8; 8];
        gas.copy_from_slice(&data[offset..offset + 8]);
        offset += 8;
        let mut ret = [0u8; 32];
        ret.copy_from_slice(&data[offset..offset + 32]);
        offset += 32;
        let has_addr = data[offset];
        offset += 1;
        let mut addr = [0u8; RECEIPT_CONTRACT_ADDR_LEN];
        addr.copy_from_slice(&data[offset..offset + RECEIPT_CONTRACT_ADDR_LEN]);
        let contract_address = if has_addr == 1 { Some(addr) } else { None };
        Self {
            tx_id: TxId(tx_id),
            block_number: u64::from_be_bytes(bn),
            tx_index: u32::from_be_bytes(ti),
            status,
            gas_used: u64::from_be_bytes(gas),
            return_data_hash: ret,
            contract_address,
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: RECEIPT_SIZE_U32,
        is_fixed_size: true,
    };
}
