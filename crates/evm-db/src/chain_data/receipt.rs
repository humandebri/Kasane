//! どこで: chain_data のReceipt / 何を: 最小結果 + logs の保存 / なぜ: 互換性と観測のため

use crate::chain_data::codec::{encode_guarded, mark_decode_failure};
use crate::chain_data::constants::{
    HASH_LEN, MAX_LOGS_PER_TX, MAX_LOG_DATA, MAX_LOG_TOPICS, MAX_RETURN_DATA,
    RECEIPT_CONTRACT_ADDR_LEN, RECEIPT_MAX_SIZE_U32,
};
use crate::chain_data::tx::TxId;
use crate::corrupt_log::record_corrupt;
use crate::decode::{read_array, read_u32, read_u64, read_u8, read_vec};
use alloy_primitives::{Address, Bytes, Log, LogData, B256};
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;

pub type LogEntry = Log;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptLike {
    pub tx_id: TxId,
    pub block_number: u64,
    pub tx_index: u32,
    pub status: u8,
    pub gas_used: u64,
    pub effective_gas_price: u64,
    pub l1_data_fee: u128,
    pub operator_fee: u128,
    pub total_fee: u128,
    pub return_data_hash: [u8; HASH_LEN],
    pub return_data: Vec<u8>,
    pub contract_address: Option<[u8; RECEIPT_CONTRACT_ADDR_LEN]>,
    pub logs: Vec<LogEntry>,
}

impl Storable for ReceiptLike {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let encoded = match self.encode_checked() {
            Ok(value) => value,
            Err(_) => {
                record_corrupt(b"receipt_encode");
                return encode_fallback_receipt();
            }
        };
        match encode_guarded(b"receipt_encode", encoded, RECEIPT_MAX_SIZE_U32) {
            Ok(value) => value,
            Err(_) => encode_fallback_receipt(),
        }
    }
    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() > RECEIPT_MAX_SIZE_U32 as usize {
            return corrupt_receipt();
        }
        let mut offset = 0usize;
        let is_v2 = data.starts_with(&RECEIPT_V2_MAGIC);
        if is_v2 {
            offset += RECEIPT_V2_MAGIC.len();
        }
        let tx_id = match read_array::<32>(data, &mut offset) {
            Some(value) => value,
            None => return corrupt_receipt(),
        };
        let block_number = match read_u64(data, &mut offset) {
            Some(value) => value,
            None => return corrupt_receipt(),
        };
        let tx_index = match read_u32(data, &mut offset) {
            Some(value) => value,
            None => return corrupt_receipt(),
        };
        let status = match read_u8(data, &mut offset) {
            Some(value) => value,
            None => return corrupt_receipt(),
        };
        if status > 1 {
            return corrupt_receipt();
        }
        let gas_used = match read_u64(data, &mut offset) {
            Some(value) => value,
            None => return corrupt_receipt(),
        };
        let effective_gas_price = match read_u64(data, &mut offset) {
            Some(value) => value,
            None => return corrupt_receipt(),
        };
        let (l1_data_fee, operator_fee, total_fee) = if is_v2 {
            let l1_data_fee = match read_array::<16>(data, &mut offset) {
                Some(value) => u128::from_be_bytes(value),
                None => return corrupt_receipt(),
            };
            let operator_fee = match read_array::<16>(data, &mut offset) {
                Some(value) => u128::from_be_bytes(value),
                None => return corrupt_receipt(),
            };
            let total_fee = match read_array::<16>(data, &mut offset) {
                Some(value) => u128::from_be_bytes(value),
                None => return corrupt_receipt(),
            };
            (l1_data_fee, operator_fee, total_fee)
        } else {
            (0, 0, 0)
        };
        let return_data_hash = match read_array::<32>(data, &mut offset) {
            Some(value) => value,
            None => return corrupt_receipt(),
        };
        let return_data = match read_vec(data, &mut offset, MAX_RETURN_DATA) {
            Some(value) => value,
            None => return corrupt_receipt(),
        };
        let has_contract = match read_u8(data, &mut offset) {
            Some(value) => value,
            None => return corrupt_receipt(),
        };
        let contract_address = if has_contract == 0 {
            if read_array::<RECEIPT_CONTRACT_ADDR_LEN>(data, &mut offset).is_none() {
                return corrupt_receipt();
            }
            None
        } else {
            let raw = match read_array::<RECEIPT_CONTRACT_ADDR_LEN>(data, &mut offset) {
                Some(value) => value,
                None => return corrupt_receipt(),
            };
            Some(raw)
        };
        let logs_len = match read_u32(data, &mut offset) {
            Some(value) => value,
            None => return corrupt_receipt(),
        };
        if logs_len as usize > MAX_LOGS_PER_TX {
            return corrupt_receipt();
        }
        let mut logs = Vec::with_capacity(logs_len as usize);
        for _ in 0..logs_len {
            let address = match read_array::<20>(data, &mut offset) {
                Some(value) => value,
                None => return corrupt_receipt(),
            };
            let topics_len = match read_u32(data, &mut offset) {
                Some(value) => value,
                None => return corrupt_receipt(),
            };
            if topics_len as usize > MAX_LOG_TOPICS {
                return corrupt_receipt();
            }
            let mut topics = Vec::with_capacity(topics_len as usize);
            for _ in 0..topics_len {
                let topic = match read_array::<32>(data, &mut offset) {
                    Some(value) => value,
                    None => return corrupt_receipt(),
                };
                topics.push(B256::from(topic));
            }
            let data = match read_vec(data, &mut offset, MAX_LOG_DATA) {
                Some(value) => value,
                None => return corrupt_receipt(),
            };
            let log_data = match LogData::new(topics, Bytes::from(data)) {
                Some(value) => value,
                None => return corrupt_receipt(),
            };
            let log = Log {
                address: Address::from(address),
                data: log_data,
            };
            logs.push(log);
        }
        ReceiptLike {
            tx_id: TxId(tx_id),
            block_number,
            tx_index,
            status,
            gas_used,
            effective_gas_price,
            l1_data_fee,
            operator_fee,
            total_fee,
            return_data_hash,
            return_data,
            contract_address,
            logs,
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: RECEIPT_MAX_SIZE_U32,
        is_fixed_size: false,
    };
}

impl ReceiptLike {
    fn encode_checked(&self) -> Result<Vec<u8>, ReceiptEncodeError> {
        if self.status > 1 {
            return Err(ReceiptEncodeError::InvalidStatus);
        }
        if self.return_data.len() > MAX_RETURN_DATA {
            return Err(ReceiptEncodeError::ReturnDataTooLarge);
        }
        if self.logs.len() > MAX_LOGS_PER_TX {
            return Err(ReceiptEncodeError::TooManyLogs);
        }
        let mut out = Vec::with_capacity(96);
        out.extend_from_slice(&RECEIPT_V2_MAGIC);
        out.extend_from_slice(&self.tx_id.0);
        out.extend_from_slice(&self.block_number.to_be_bytes());
        out.extend_from_slice(&self.tx_index.to_be_bytes());
        out.push(self.status);
        out.extend_from_slice(&self.gas_used.to_be_bytes());
        out.extend_from_slice(&self.effective_gas_price.to_be_bytes());
        out.extend_from_slice(&self.l1_data_fee.to_be_bytes());
        out.extend_from_slice(&self.operator_fee.to_be_bytes());
        out.extend_from_slice(&self.total_fee.to_be_bytes());
        out.extend_from_slice(&self.return_data_hash);
        let data_len = u32::try_from(self.return_data.len())
            .map_err(|_| ReceiptEncodeError::LengthOverflow)?;
        out.extend_from_slice(&data_len.to_be_bytes());
        out.extend_from_slice(&self.return_data);
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
        let logs_len = u32::try_from(self.logs.len()).map_err(|_| ReceiptEncodeError::LengthOverflow)?;
        out.extend_from_slice(&logs_len.to_be_bytes());
        for log in self.logs.iter() {
            let topics = log.data.topics();
            if topics.len() > MAX_LOG_TOPICS {
                return Err(ReceiptEncodeError::TooManyTopics);
            }
            let data = log.data.data.as_ref();
            if data.len() > MAX_LOG_DATA {
                return Err(ReceiptEncodeError::LogDataTooLarge);
            }
            out.extend_from_slice(log.address.as_ref());
            let topics_len =
                u32::try_from(topics.len()).map_err(|_| ReceiptEncodeError::LengthOverflow)?;
            out.extend_from_slice(&topics_len.to_be_bytes());
            for topic in topics.iter() {
                out.extend_from_slice(topic.as_ref());
            }
            let data_len =
                u32::try_from(data.len()).map_err(|_| ReceiptEncodeError::LengthOverflow)?;
            out.extend_from_slice(&data_len.to_be_bytes());
            out.extend_from_slice(data);
        }
        Ok(out)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReceiptEncodeError {
    InvalidStatus,
    ReturnDataTooLarge,
    TooManyLogs,
    TooManyTopics,
    LogDataTooLarge,
    LengthOverflow,
}

fn encode_fallback_receipt() -> Cow<'static, [u8]> {
    let receipt = corrupt_receipt();
    let encoded = receipt.encode_checked().unwrap_or_else(|_| {
        let mut out = Vec::with_capacity(96);
        out.extend_from_slice(&RECEIPT_V2_MAGIC);
        out.extend_from_slice(&[0u8; 32]);
        out.extend_from_slice(&0u64.to_be_bytes());
        out.extend_from_slice(&0u32.to_be_bytes());
        out.push(0u8);
        out.extend_from_slice(&0u64.to_be_bytes());
        out.extend_from_slice(&0u64.to_be_bytes());
        out.extend_from_slice(&0u128.to_be_bytes());
        out.extend_from_slice(&0u128.to_be_bytes());
        out.extend_from_slice(&0u128.to_be_bytes());
        out.extend_from_slice(&[0u8; 32]);
        out.extend_from_slice(&0u32.to_be_bytes());
        out.push(0u8);
        out.extend_from_slice(&[0u8; RECEIPT_CONTRACT_ADDR_LEN]);
        out.extend_from_slice(&0u32.to_be_bytes());
        out
    });
    match encode_guarded(b"receipt_encode", encoded, RECEIPT_MAX_SIZE_U32) {
        Ok(value) => value,
        Err(_) => Cow::Owned(vec![0u8; RECEIPT_MAX_SIZE_U32 as usize]),
    }
}

fn corrupt_receipt() -> ReceiptLike {
    mark_decode_failure(b"receipt", true);
    ReceiptLike {
        tx_id: TxId([0u8; 32]),
        block_number: 0,
        tx_index: 0,
        status: 0,
        gas_used: 0,
        effective_gas_price: 0,
        l1_data_fee: 0,
        operator_fee: 0,
        total_fee: 0,
        return_data_hash: [0u8; HASH_LEN],
        return_data: Vec::new(),
        contract_address: None,
        logs: Vec::new(),
    }
}

const RECEIPT_V2_MAGIC: [u8; 8] = *b"rcptv2\0\x02";
