//! どこで: stableのBlob格納 / 何を: arena + alloc_table + free_list / なぜ: 再利用可能な基盤を先に固定するため

use crate::blob_ptr::BlobPtr;
use crate::size_class::{smallest_class, SizeClassError};
use crate::memory::VMem;
use ic_stable_structures::storable::Bound;
use ic_stable_structures::{Memory, StableBTreeMap, StableCell, Storable};
use std::borrow::Cow;

const WASM_PAGE_SIZE: u64 = 65536;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlobState {
    Used = 1,
    Quarantine = 2,
    Free = 3,
}

impl BlobState {
    fn to_u8(self) -> u8 {
        match self {
            BlobState::Used => 1,
            BlobState::Quarantine => 2,
            BlobState::Free => 3,
        }
    }

    fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(BlobState::Used),
            2 => Some(BlobState::Quarantine),
            3 => Some(BlobState::Free),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct AllocKey {
    pub class: u32,
    pub offset: u64,
}

impl Storable for AllocKey {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut out = [0u8; 12];
        out[0..4].copy_from_slice(&self.class.to_be_bytes());
        out[4..12].copy_from_slice(&self.offset.to_be_bytes());
        Cow::Owned(out.to_vec())
    }

    fn into_bytes(self) -> Vec<u8> {
        let mut out = [0u8; 12];
        out[0..4].copy_from_slice(&self.class.to_be_bytes());
        out[4..12].copy_from_slice(&self.offset.to_be_bytes());
        out.to_vec()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != 12 {
            return Self { class: 0, offset: 0 };
        }
        let mut class = [0u8; 4];
        class.copy_from_slice(&data[0..4]);
        let mut offset = [0u8; 8];
        offset.copy_from_slice(&data[4..12]);
        Self {
            class: u32::from_be_bytes(class),
            offset: u64::from_be_bytes(offset),
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: 12,
        is_fixed_size: true,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AllocEntry {
    pub gen: u32,
    pub state: BlobState,
}

impl Storable for AllocEntry {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut out = [0u8; 8];
        out[0..4].copy_from_slice(&self.gen.to_be_bytes());
        out[4] = self.state.to_u8();
        Cow::Owned(out.to_vec())
    }

    fn into_bytes(self) -> Vec<u8> {
        let mut out = [0u8; 8];
        out[0..4].copy_from_slice(&self.gen.to_be_bytes());
        out[4] = self.state.to_u8();
        out.to_vec()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != 8 {
            return Self {
                gen: 0,
                state: BlobState::Free,
            };
        }
        let mut gen = [0u8; 4];
        gen.copy_from_slice(&data[0..4]);
        let state = BlobState::from_u8(data[4]).unwrap_or(BlobState::Free);
        Self {
            gen: u32::from_be_bytes(gen),
            state,
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: 8,
        is_fixed_size: true,
    };
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BlobError {
    SizeClass(SizeClassError),
    LengthTooLarge,
    Overflow,
    GrowFailed,
    MissingAllocEntry,
    InvalidState,
    InvalidPointer,
    LengthMismatch,
    DuplicateFree,
}

pub struct BlobStore {
    arena: VMem,
    arena_end: StableCell<u64, VMem>,
    alloc_table: StableBTreeMap<AllocKey, AllocEntry, VMem>,
    free_list_by_class: StableBTreeMap<AllocKey, (), VMem>,
}

impl BlobStore {
    pub fn new(
        arena: VMem,
        arena_end: StableCell<u64, VMem>,
        alloc_table: StableBTreeMap<AllocKey, AllocEntry, VMem>,
        free_list_by_class: StableBTreeMap<AllocKey, (), VMem>,
    ) -> Self {
        Self {
            arena,
            arena_end,
            alloc_table,
            free_list_by_class,
        }
    }

    pub fn allocate(&mut self, len: usize) -> Result<BlobPtr, BlobError> {
        let class = smallest_class(len).map_err(BlobError::SizeClass)?;
        let len_u32 = u32::try_from(len).map_err(|_| BlobError::LengthTooLarge)?;
        let offset = match self.pop_free(class) {
            Some(value) => {
                let key = AllocKey { class, offset: value };
                let mut entry = self
                    .alloc_table
                    .get(&key)
                    .ok_or(BlobError::MissingAllocEntry)?;
                if entry.state != BlobState::Free {
                    return Err(BlobError::InvalidState);
                }
                entry.gen = entry.gen.checked_add(1).ok_or(BlobError::Overflow)?;
                entry.state = BlobState::Used;
                self.alloc_table.insert(key, entry);
                value
            }
            None => {
                let current = *self.arena_end.get();
                let class_u64 = u64::from(class);
                let end = current.checked_add(class_u64).ok_or(BlobError::Overflow)?;
                self.ensure_capacity(end)?;
                self.arena_end.set(end);
                let key = AllocKey {
                    class,
                    offset: current,
                };
                let entry = AllocEntry {
                    gen: 1,
                    state: BlobState::Used,
                };
                self.alloc_table.insert(key, entry);
                current
            }
        };
        Ok(BlobPtr {
            offset,
            len: len_u32,
            class,
            gen: self.current_gen(class, offset)?,
        })
    }

    pub fn store_bytes(&mut self, data: &[u8]) -> Result<BlobPtr, BlobError> {
        let ptr = self.allocate(data.len())?;
        self.write(&ptr, data)?;
        Ok(ptr)
    }

    pub fn read(&self, ptr: &BlobPtr) -> Result<Vec<u8>, BlobError> {
        let entry = self
            .alloc_table
            .get(&AllocKey {
                class: ptr.class,
                offset: ptr.offset,
            })
            .ok_or(BlobError::MissingAllocEntry)?;
        if entry.gen != ptr.gen {
            return Err(BlobError::InvalidPointer);
        }
        if entry.state == BlobState::Free {
            return Err(BlobError::InvalidState);
        }
        let len_u64 = u64::from(ptr.len);
        let end = ptr.offset.checked_add(len_u64).ok_or(BlobError::Overflow)?;
        if end > *self.arena_end.get() {
            return Err(BlobError::InvalidPointer);
        }
        let mut out = vec![0u8; usize::try_from(ptr.len).map_err(|_| BlobError::LengthTooLarge)?];
        self.arena.read(ptr.offset, &mut out);
        Ok(out)
    }

    pub fn write(&mut self, ptr: &BlobPtr, data: &[u8]) -> Result<(), BlobError> {
        if data.len() != usize::try_from(ptr.len).map_err(|_| BlobError::LengthTooLarge)? {
            return Err(BlobError::LengthMismatch);
        }
        let entry = self
            .alloc_table
            .get(&AllocKey {
                class: ptr.class,
                offset: ptr.offset,
            })
            .ok_or(BlobError::MissingAllocEntry)?;
        if entry.gen != ptr.gen || entry.state != BlobState::Used {
            return Err(BlobError::InvalidState);
        }
        let end = ptr
            .offset
            .checked_add(u64::from(ptr.class))
            .ok_or(BlobError::Overflow)?;
        self.ensure_capacity(end)?;
        self.arena.write(ptr.offset, data);
        Ok(())
    }

    pub fn mark_quarantine(&mut self, ptr: &BlobPtr) -> Result<(), BlobError> {
        let key = AllocKey {
            class: ptr.class,
            offset: ptr.offset,
        };
        let mut entry = self.alloc_table.get(&key).ok_or(BlobError::MissingAllocEntry)?;
        if entry.gen != ptr.gen {
            return Err(BlobError::InvalidPointer);
        }
        if entry.state == BlobState::Quarantine {
            return Ok(());
        }
        if entry.state != BlobState::Used {
            return Err(BlobError::InvalidState);
        }
        entry.state = BlobState::Quarantine;
        self.alloc_table.insert(key, entry);
        Ok(())
    }

    pub fn mark_free(&mut self, ptr: &BlobPtr) -> Result<(), BlobError> {
        let key = AllocKey {
            class: ptr.class,
            offset: ptr.offset,
        };
        let mut entry = self.alloc_table.get(&key).ok_or(BlobError::MissingAllocEntry)?;
        if entry.gen != ptr.gen {
            return Err(BlobError::InvalidPointer);
        }
        if entry.state == BlobState::Free {
            return Ok(());
        }
        if entry.state != BlobState::Quarantine {
            return Err(BlobError::InvalidState);
        }
        if self.free_list_by_class.get(&key).is_some() {
            return Err(BlobError::DuplicateFree);
        }
        entry.state = BlobState::Free;
        self.alloc_table.insert(key, entry);
        self.free_list_by_class.insert(key, ());
        Ok(())
    }

    fn pop_free(&mut self, class: u32) -> Option<u64> {
        let start = AllocKey { class, offset: 0 };
        let end = AllocKey {
            class,
            offset: u64::MAX,
        };
        let mut iter = self.free_list_by_class.range(start..=end);
        let entry = iter.next()?;
        let key = entry.key().clone();
        self.free_list_by_class.remove(&key);
        Some(key.offset)
    }

    fn current_gen(&self, class: u32, offset: u64) -> Result<u32, BlobError> {
        let entry = self
            .alloc_table
            .get(&AllocKey { class, offset })
            .ok_or(BlobError::MissingAllocEntry)?;
        Ok(entry.gen)
    }

    fn ensure_capacity(&self, end_offset: u64) -> Result<(), BlobError> {
        let current_pages = self.arena.size();
        let required = pages_required(end_offset)?;
        if required > current_pages {
            let grow = required.saturating_sub(current_pages);
            let prev = self.arena.grow(grow);
            if prev < 0 {
                return Err(BlobError::GrowFailed);
            }
        }
        Ok(())
    }
}

fn pages_required(end_offset: u64) -> Result<u64, BlobError> {
    if end_offset == 0 {
        return Ok(0);
    }
    let add = WASM_PAGE_SIZE
        .checked_sub(1)
        .ok_or(BlobError::Overflow)?;
    let sum = end_offset.checked_add(add).ok_or(BlobError::Overflow)?;
    Ok(sum / WASM_PAGE_SIZE)
}
