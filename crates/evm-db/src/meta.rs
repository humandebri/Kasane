//! どこで: META領域 / 何を: schema版管理とmigration状態の保持 / なぜ: upgradeを安全に再実行可能にするため

use crate::corrupt_log::record_corrupt;
use crate::memory::{get_memory, AppMemoryId, VMem};
use ic_stable_structures::storable::Bound;
use ic_stable_structures::{StableCell, Storable};
use std::borrow::Cow;
use tracing::warn;
use zerocopy::byteorder::big_endian::{U32, U64};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

const META_MAGIC: [u8; 4] = *b"EVM0";
const META_LAYOUT_VERSION: u32 = 2;
const META_LEGACY_SIZE: usize = 40;
const META_SIZE: usize = 64;
pub const CURRENT_SCHEMA_VERSION: u32 = 5;
#[allow(dead_code)]
const META_SCHEMA_STRING: &str = "mem:0..4|keys:v2|ic_tx:rlp-fixed|merkle:v1|env:v1";
// Keccak-256(META_SCHEMA_STRING)
const META_SCHEMA_HASH: [u8; 32] = [
    0x02, 0x8f, 0x59, 0xb7, 0xbf, 0xf9, 0xda, 0x2d, 0xf9, 0x58, 0xa9, 0x22, 0xd7, 0x61, 0xad, 0xb1,
    0x36, 0xe2, 0x8d, 0xb7, 0x45, 0xf4, 0xf4, 0xaf, 0x25, 0xf7, 0x7f, 0x60, 0xa4, 0x9b, 0xf0, 0x7b,
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

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned,
)]
#[repr(C)]
struct SchemaMigrationWire {
    phase: u8,
    cursor_key_set: u8,
    _pad0: [u8; 6],
    cursor: U64,
    from_version: U32,
    to_version: U32,
    last_error: U32,
    cursor_key: [u8; 32],
    _pad1: [u8; 4],
}

impl SchemaMigrationWire {
    fn new(state: &SchemaMigrationState) -> Self {
        Self {
            phase: state.phase as u8,
            cursor_key_set: u8::from(state.cursor_key_set),
            _pad0: [0u8; 6],
            cursor: U64::new(state.cursor),
            from_version: U32::new(state.from_version),
            to_version: U32::new(state.to_version),
            last_error: U32::new(state.last_error),
            cursor_key: state.cursor_key,
            _pad1: [0u8; 4],
        }
    }
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
        let wire = SchemaMigrationWire::new(self);
        Cow::Owned(wire.as_bytes().to_vec())
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
        if data.len() == SCHEMA_MIGRATION_SIZE {
            let wire = match SchemaMigrationWire::read_from_bytes(data) {
                Ok(value) => value,
                Err(_) => {
                    record_corrupt(b"schema_migration_state");
                    return Self::done();
                }
            };
            return Self {
                phase: SchemaMigrationPhase::from_u8(wire.phase),
                cursor: wire.cursor.get(),
                from_version: wire.from_version.get(),
                to_version: wire.to_version.get(),
                last_error: wire.last_error.get(),
                cursor_key_set: wire.cursor_key_set != 0,
                cursor_key: wire.cursor_key,
            };
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

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned,
)]
#[repr(C)]
struct MetaWire {
    magic: [u8; 4],
    layout_version: U32,
    schema_hash: [u8; 32],
    schema_version: U32,
    needs_migration: u8,
    active_tx_locs_v3: u8,
    _pad0: [u8; 2],
    last_migration_from: U32,
    last_migration_to: U32,
    last_migration_ts: U64,
}

impl MetaWire {
    fn new(meta: &Meta) -> Self {
        Self {
            magic: meta.magic,
            layout_version: U32::new(meta.layout_version),
            schema_hash: meta.schema_hash,
            schema_version: U32::new(meta.schema_version),
            needs_migration: u8::from(meta.needs_migration),
            active_tx_locs_v3: u8::from(meta.active_tx_locs_v3),
            _pad0: [0u8; 2],
            last_migration_from: U32::new(meta.last_migration_from),
            last_migration_to: U32::new(meta.last_migration_to),
            last_migration_ts: U64::new(meta.last_migration_ts),
        }
    }
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
        let wire = MetaWire::new(self);
        Cow::Owned(wire.as_bytes().to_vec())
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != META_LEGACY_SIZE && data.len() != META_SIZE {
            record_corrupt(b"meta");
            return fail_closed_meta();
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
        let wire = match MetaWire::read_from_bytes(data) {
            Ok(value) => value,
            Err(_) => {
                record_corrupt(b"meta");
                return fail_closed_meta();
            }
        };
        Self {
            magic: wire.magic,
            layout_version: wire.layout_version.get(),
            schema_hash: wire.schema_hash,
            schema_version: wire.schema_version.get(),
            needs_migration: wire.needs_migration != 0,
            active_tx_locs_v3: wire.active_tx_locs_v3 != 0,
            last_migration_from: wire.last_migration_from.get(),
            last_migration_to: wire.last_migration_to.get(),
            last_migration_ts: wire.last_migration_ts.get(),
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: META_SIZE as u32,
        is_fixed_size: true,
    };
}

fn fail_closed_meta() -> Meta {
    Meta {
        schema_version: 0,
        needs_migration: true,
        ..Meta::new()
    }
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
