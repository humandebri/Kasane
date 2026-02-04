//! どこで: OP向けL1情報のstable保存 / 何を: paramsとsnapshotを分離保持 / なぜ: ブロック境界で一貫適用するため

use crate::corrupt_log::record_corrupt;
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;

pub const L1_BLOCK_INFO_PARAMS_SIZE_U32: u32 = 96;
pub const L1_BLOCK_INFO_SNAPSHOT_SIZE_U32: u32 = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct L1BlockInfoParamsV1 {
    pub schema_version: u32,
    pub spec_id: u8,
    pub empty_ecotone_scalars: bool,
    pub l1_fee_overhead: u128,
    pub l1_base_fee_scalar: u128,
    pub l1_blob_base_fee_scalar: u128,
    pub operator_fee_scalar: u128,
    pub operator_fee_constant: u128,
}

impl L1BlockInfoParamsV1 {
    pub fn new() -> Self {
        Self {
            schema_version: 1,
            spec_id: 101, // REGOLITH
            empty_ecotone_scalars: false,
            l1_fee_overhead: 0,
            l1_base_fee_scalar: 1_000_000,
            l1_blob_base_fee_scalar: 0,
            operator_fee_scalar: 0,
            operator_fee_constant: 0,
        }
    }
}

impl Default for L1BlockInfoParamsV1 {
    fn default() -> Self {
        Self::new()
    }
}

impl Storable for L1BlockInfoParamsV1 {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut out = [0u8; L1_BLOCK_INFO_PARAMS_SIZE_U32 as usize];
        out[0..4].copy_from_slice(&self.schema_version.to_be_bytes());
        out[4] = self.spec_id;
        out[5] = u8::from(self.empty_ecotone_scalars);
        out[8..24].copy_from_slice(&self.l1_fee_overhead.to_be_bytes());
        out[24..40].copy_from_slice(&self.l1_base_fee_scalar.to_be_bytes());
        out[40..56].copy_from_slice(&self.l1_blob_base_fee_scalar.to_be_bytes());
        out[56..72].copy_from_slice(&self.operator_fee_scalar.to_be_bytes());
        out[72..88].copy_from_slice(&self.operator_fee_constant.to_be_bytes());
        Cow::Owned(out.to_vec())
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != L1_BLOCK_INFO_PARAMS_SIZE_U32 as usize {
            record_corrupt(b"l1_params");
            return Self::new();
        }
        let mut schema = [0u8; 4];
        schema.copy_from_slice(&data[0..4]);
        let spec_id = data[4];
        let empty_ecotone_scalars = data[5] != 0;
        let mut l1_fee_overhead = [0u8; 16];
        l1_fee_overhead.copy_from_slice(&data[8..24]);
        let mut l1_base_fee_scalar = [0u8; 16];
        l1_base_fee_scalar.copy_from_slice(&data[24..40]);
        let mut l1_blob_base_fee_scalar = [0u8; 16];
        l1_blob_base_fee_scalar.copy_from_slice(&data[40..56]);
        let mut operator_fee_scalar = [0u8; 16];
        operator_fee_scalar.copy_from_slice(&data[56..72]);
        let mut operator_fee_constant = [0u8; 16];
        operator_fee_constant.copy_from_slice(&data[72..88]);
        Self {
            schema_version: u32::from_be_bytes(schema),
            spec_id,
            empty_ecotone_scalars,
            l1_fee_overhead: u128::from_be_bytes(l1_fee_overhead),
            l1_base_fee_scalar: u128::from_be_bytes(l1_base_fee_scalar),
            l1_blob_base_fee_scalar: u128::from_be_bytes(l1_blob_base_fee_scalar),
            operator_fee_scalar: u128::from_be_bytes(operator_fee_scalar),
            operator_fee_constant: u128::from_be_bytes(operator_fee_constant),
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: L1_BLOCK_INFO_PARAMS_SIZE_U32,
        is_fixed_size: true,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct L1BlockInfoSnapshotV1 {
    pub schema_version: u32,
    pub enabled: bool,
    pub l1_block_number: u64,
    pub l1_base_fee: u128,
    pub l1_blob_base_fee: u128,
}

impl L1BlockInfoSnapshotV1 {
    pub fn new() -> Self {
        Self {
            schema_version: 1,
            enabled: false,
            l1_block_number: 0,
            l1_base_fee: 0,
            l1_blob_base_fee: 0,
        }
    }
}

impl Default for L1BlockInfoSnapshotV1 {
    fn default() -> Self {
        Self::new()
    }
}

impl Storable for L1BlockInfoSnapshotV1 {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut out = [0u8; L1_BLOCK_INFO_SNAPSHOT_SIZE_U32 as usize];
        out[0..4].copy_from_slice(&self.schema_version.to_be_bytes());
        out[4] = u8::from(self.enabled);
        out[8..16].copy_from_slice(&self.l1_block_number.to_be_bytes());
        out[16..32].copy_from_slice(&self.l1_base_fee.to_be_bytes());
        out[32..48].copy_from_slice(&self.l1_blob_base_fee.to_be_bytes());
        Cow::Owned(out.to_vec())
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != L1_BLOCK_INFO_SNAPSHOT_SIZE_U32 as usize {
            record_corrupt(b"l1_snapshot");
            return Self::new();
        }
        let mut schema = [0u8; 4];
        schema.copy_from_slice(&data[0..4]);
        let enabled = data[4] != 0;
        let mut l1_block = [0u8; 8];
        l1_block.copy_from_slice(&data[8..16]);
        let mut l1_base_fee = [0u8; 16];
        l1_base_fee.copy_from_slice(&data[16..32]);
        let mut l1_blob_base_fee = [0u8; 16];
        l1_blob_base_fee.copy_from_slice(&data[32..48]);
        Self {
            schema_version: u32::from_be_bytes(schema),
            enabled,
            l1_block_number: u64::from_be_bytes(l1_block),
            l1_base_fee: u128::from_be_bytes(l1_base_fee),
            l1_blob_base_fee: u128::from_be_bytes(l1_blob_base_fee),
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: L1_BLOCK_INFO_SNAPSHOT_SIZE_U32,
        is_fixed_size: true,
    };
}
