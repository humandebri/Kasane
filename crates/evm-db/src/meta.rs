//! どこで: META領域 / 何を: schema版管理とmigration状態の保持 / なぜ: upgradeを安全に再実行可能にするため

use crate::corrupt_log::record_corrupt;
use crate::memory::{get_memory, AppMemoryId, VMem};
use ic_stable_structures::storable::Bound;
use ic_stable_structures::{StableCell, Storable};
use std::borrow::Cow;
use tracing::warn;

const META_MAGIC: [u8; 4] = *b"EVM0";
const META_LAYOUT_VERSION: u32 = 2;
const META_LEGACY_SIZE: usize = 40;
const META_SIZE: usize = 64;
pub const CURRENT_SCHEMA_VERSION: u32 = 3;
#[allow(dead_code)]
const META_SCHEMA_STRING: &str = "mem:0..4|keys:v1|ic_tx:rlp-fixed|merkle:v1|env:v1";
// Keccak-256(META_SCHEMA_STRING)
const META_SCHEMA_HASH: [u8; 32] = [
    0x6d, 0x56, 0xd9, 0x15, 0xe8, 0x9e, 0x5d, 0x97, 0xd7, 0xbe, 0xc4, 0xe0, 0x52, 0xa0, 0xc7, 0xb1,
    0xc1, 0xd3, 0x38, 0x12, 0x71, 0x77, 0x5e, 0xdd, 0x07, 0xc2, 0xc5, 0xa4, 0x77, 0xd5, 0xa8, 0x1b,
];
const SCHEMA_MIGRATION_LEGACY_SIZE: usize = 32;
const SCHEMA_MIGRATION_SIZE: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum SchemaMigrationPhase {
    Init = 0,
    Scan = 1,
    Rewrite = 2,
    Verify = 3,
    Done = 4,
    Error = 5,
}

impl SchemaMigrationPhase {
    fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Init,
            1 => Self::Scan,
            2 => Self::Rewrite,
            3 => Self::Verify,
            4 => Self::Done,
            5 => Self::Error,
            _ => Self::Error,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SchemaMigrationState {
    pub phase: SchemaMigrationPhase,
    pub cursor: u64,
    pub from_version: u32,
    pub to_version: u32,
    pub last_error: u32,
    pub cursor_key_set: bool,
    pub cursor_key: [u8; 32],
}

impl SchemaMigrationState {
    pub fn done() -> Self {
        Self {
            phase: SchemaMigrationPhase::Done,
            cursor: 0,
            from_version: CURRENT_SCHEMA_VERSION,
            to_version: CURRENT_SCHEMA_VERSION,
            last_error: 0,
            cursor_key_set: false,
            cursor_key: [0u8; 32],
        }
    }
}

impl Storable for SchemaMigrationState {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut out = [0u8; SCHEMA_MIGRATION_SIZE];
        out[0] = self.phase as u8;
        out[1] = u8::from(self.cursor_key_set);
        out[8..16].copy_from_slice(&self.cursor.to_be_bytes());
        out[16..20].copy_from_slice(&self.from_version.to_be_bytes());
        out[20..24].copy_from_slice(&self.to_version.to_be_bytes());
        out[24..28].copy_from_slice(&self.last_error.to_be_bytes());
        out[28..60].copy_from_slice(&self.cursor_key);
        Cow::Owned(out.to_vec())
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != SCHEMA_MIGRATION_LEGACY_SIZE && data.len() != SCHEMA_MIGRATION_SIZE {
            record_corrupt(b"schema_migration_state");
            return Self::done();
        }
        let mut cursor = [0u8; 8];
        cursor.copy_from_slice(&data[8..16]);
        let mut from_version = [0u8; 4];
        from_version.copy_from_slice(&data[16..20]);
        let mut to_version = [0u8; 4];
        to_version.copy_from_slice(&data[20..24]);
        let mut last_error = [0u8; 4];
        last_error.copy_from_slice(&data[24..28]);
        let mut cursor_key = [0u8; 32];
        let cursor_key_set = if data.len() == SCHEMA_MIGRATION_SIZE {
            cursor_key.copy_from_slice(&data[28..60]);
            data[1] != 0
        } else {
            false
        };
        Self {
            phase: SchemaMigrationPhase::from_u8(data[0]),
            cursor: u64::from_be_bytes(cursor),
            from_version: u32::from_be_bytes(from_version),
            to_version: u32::from_be_bytes(to_version),
            last_error: u32::from_be_bytes(last_error),
            cursor_key_set,
            cursor_key,
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: SCHEMA_MIGRATION_SIZE as u32,
        is_fixed_size: true,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Meta {
    pub magic: [u8; 4],
    pub layout_version: u32,
    pub schema_hash: [u8; 32],
    pub schema_version: u32,
    pub needs_migration: bool,
    pub active_tx_locs_v3: bool,
    pub last_migration_from: u32,
    pub last_migration_to: u32,
    pub last_migration_ts: u64,
}

impl Meta {
    pub fn new() -> Self {
        Self {
            magic: META_MAGIC,
            layout_version: META_LAYOUT_VERSION,
            schema_hash: META_SCHEMA_HASH,
            schema_version: CURRENT_SCHEMA_VERSION,
            needs_migration: false,
            active_tx_locs_v3: false,
            last_migration_from: CURRENT_SCHEMA_VERSION,
            last_migration_to: CURRENT_SCHEMA_VERSION,
            last_migration_ts: 0,
        }
    }
}

impl Default for Meta {
    fn default() -> Self {
        Self::new()
    }
}

impl Storable for Meta {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut buf = [0u8; META_SIZE];
        buf[0..4].copy_from_slice(&self.magic);
        buf[4..8].copy_from_slice(&self.layout_version.to_be_bytes());
        buf[8..40].copy_from_slice(&self.schema_hash);
        buf[40..44].copy_from_slice(&self.schema_version.to_be_bytes());
        buf[44] = u8::from(self.needs_migration);
        buf[45] = u8::from(self.active_tx_locs_v3);
        buf[48..52].copy_from_slice(&self.last_migration_from.to_be_bytes());
        buf[52..56].copy_from_slice(&self.last_migration_to.to_be_bytes());
        buf[56..64].copy_from_slice(&self.last_migration_ts.to_be_bytes());
        Cow::Owned(buf.to_vec())
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != META_LEGACY_SIZE && data.len() != META_SIZE {
            record_corrupt(b"meta");
            return Meta::new();
        }
        let mut magic = [0u8; 4];
        let mut schema_hash = [0u8; 32];
        magic.copy_from_slice(&data[0..4]);
        let layout_version = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        schema_hash.copy_from_slice(&data[8..40]);
        if data.len() == META_LEGACY_SIZE {
            return Self {
                magic,
                layout_version,
                schema_hash,
                schema_version: 1,
                needs_migration: true,
                active_tx_locs_v3: false,
                last_migration_from: 1,
                last_migration_to: 1,
                last_migration_ts: 0,
            };
        }
        let schema_version = u32::from_be_bytes([data[40], data[41], data[42], data[43]]);
        let needs_migration = data[44] != 0;
        let active_tx_locs_v3 = data[45] != 0;
        let last_migration_from = u32::from_be_bytes([data[48], data[49], data[50], data[51]]);
        let last_migration_to = u32::from_be_bytes([data[52], data[53], data[54], data[55]]);
        let last_migration_ts = u64::from_be_bytes([
            data[56], data[57], data[58], data[59], data[60], data[61], data[62], data[63],
        ]);
        Self {
            magic,
            layout_version,
            schema_hash,
            schema_version,
            needs_migration,
            active_tx_locs_v3,
            last_migration_from,
            last_migration_to,
            last_migration_ts,
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: META_SIZE as u32,
        is_fixed_size: true,
    };
}

fn init_meta_cell() -> StableCell<Meta, VMem> {
    StableCell::init(get_memory(AppMemoryId::Meta), Meta::new())
}

pub fn init_meta_or_trap() {
    let _ = ensure_meta_initialized();
}

pub fn ensure_meta_initialized() -> Meta {
    let mut cell = init_meta_cell();
    let mut meta = *cell.get();
    let mut dirty = false;
    if meta.magic != META_MAGIC {
        record_corrupt(b"meta_magic");
        warn!("meta: magic mismatch; repaired");
        meta.magic = META_MAGIC;
        meta.needs_migration = true;
        dirty = true;
    }
    if meta.layout_version != META_LAYOUT_VERSION {
        record_corrupt(b"meta_layout");
        warn!("meta: layout_version mismatch; repaired");
        meta.layout_version = META_LAYOUT_VERSION;
        meta.needs_migration = true;
        dirty = true;
    }
    if meta.schema_hash != META_SCHEMA_HASH {
        record_corrupt(b"meta_schema_hash");
        warn!("meta: schema_hash mismatch; repaired");
        meta.schema_hash = META_SCHEMA_HASH;
        meta.needs_migration = true;
        dirty = true;
    }
    if meta.schema_version == 0 {
        record_corrupt(b"meta_schema_version");
        warn!("meta: schema_version=0; repaired");
        meta.schema_version = 1;
        meta.needs_migration = true;
        dirty = true;
    }
    if dirty {
        let _ = cell.set(meta);
    }
    meta
}

pub fn get_meta() -> Meta {
    ensure_meta_initialized()
}

pub fn set_meta(meta: Meta) {
    let mut cell = init_meta_cell();
    let _ = cell.set(meta);
}

pub fn mark_migration_applied(from: u32, to: u32, ts: u64) {
    let mut meta = ensure_meta_initialized();
    meta.schema_version = to;
    meta.needs_migration = false;
    meta.last_migration_from = from;
    meta.last_migration_to = to;
    meta.last_migration_ts = ts;
    set_meta(meta);
}

pub fn tx_locs_v3_active() -> bool {
    ensure_meta_initialized().active_tx_locs_v3
}

pub fn set_tx_locs_v3_active(value: bool) {
    let mut meta = ensure_meta_initialized();
    meta.active_tx_locs_v3 = value;
    set_meta(meta);
}

pub fn set_needs_migration(value: bool) {
    let mut meta = ensure_meta_initialized();
    meta.needs_migration = value;
    set_meta(meta);
}

pub fn needs_migration() -> bool {
    ensure_meta_initialized().needs_migration
}

pub fn schema_version() -> u32 {
    ensure_meta_initialized().schema_version
}

pub fn current_schema_version() -> u32 {
    CURRENT_SCHEMA_VERSION
}

pub fn last_migration() -> (u32, u32, u64) {
    let meta = ensure_meta_initialized();
    (
        meta.last_migration_from,
        meta.last_migration_to,
        meta.last_migration_ts,
    )
}

pub fn set_schema_version(version: u32) {
    let mut meta = ensure_meta_initialized();
    meta.schema_version = version;
    set_meta(meta);
}

pub fn clear_needs_migration() {
    set_needs_migration(false);
}

pub fn set_needs_migration_and_schema(version: u32) {
    let mut meta = ensure_meta_initialized();
    meta.schema_version = version;
    meta.needs_migration = true;
    set_meta(meta);
}

pub fn is_schema_supported() -> bool {
    let version = schema_version();
    version <= current_schema_version()
}

pub fn migration_pending() -> bool {
    let meta = ensure_meta_initialized();
    meta.needs_migration || meta.schema_version < current_schema_version()
}

pub fn mark_meta_needs_migration_if_unsupported() {
    let version = schema_version();
    if version > current_schema_version() {
        set_needs_migration(true);
    }
}

fn init_schema_migration_cell() -> StableCell<SchemaMigrationState, VMem> {
    StableCell::init(
        get_memory(AppMemoryId::Reserved38),
        SchemaMigrationState::done(),
    )
}

pub fn schema_migration_state() -> SchemaMigrationState {
    *init_schema_migration_cell().get()
}

pub fn set_schema_migration_state(next: SchemaMigrationState) {
    let mut cell = init_schema_migration_cell();
    let _ = cell.set(next);
}
