//! Diagnostic: a trait default's concrete return type is not every impl's return type.
use std::ops::Deref;

pub trait MemoryTr {
    fn slice(&self) -> &[u8];

    fn slice_len(&self) -> impl Deref<Target = [u8]> + '_ {
        self.slice()
    }
}

pub struct DefaultMemory<'a> {
    pub bytes: &'a [u8],
}

impl MemoryTr for DefaultMemory<'_> {
    fn slice(&self) -> &[u8] {
        self.bytes
    }
}

pub struct OverrideMemory<'a> {
    pub bytes: &'a [u8],
}

impl MemoryTr for OverrideMemory<'_> {
    fn slice(&self) -> &[u8] {
        self.bytes
    }

    fn slice_len(&self) -> impl Deref<Target = [u8]> + '_ {
        vec![7, 8, 9]
    }
}

pub fn generic_size<M: MemoryTr>(memory: &M) -> usize {
    memory.slice_len().len()
}

pub fn default_size(memory: &DefaultMemory<'_>) -> usize {
    generic_size(memory)
}

pub fn override_size(memory: &OverrideMemory<'_>) -> usize {
    generic_size(memory)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_dispatch_reads_borrowed_input() {
        assert_eq!(default_size(&DefaultMemory { bytes: &[1] }), 1);
        assert_eq!(default_size(&DefaultMemory { bytes: &[] }), 0);
    }

    #[test]
    fn override_dispatch_reads_owned_return_instead_of_input() {
        assert_eq!(override_size(&OverrideMemory { bytes: &[1] }), 3);
        assert_eq!(override_size(&OverrideMemory { bytes: &[] }), 3);
    }
}
