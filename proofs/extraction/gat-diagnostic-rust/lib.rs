// Diagnostic only: isolates the MemoryTr return shape without changing revm.
#[cfg(any(feature = "rpit", feature = "named"))]
use std::ops::Deref;

pub struct Memory<'a> {
    pub bytes: &'a [u8],
}

#[cfg(feature = "plain")]
pub trait MemoryTr {
    fn slice(&self) -> &[u8];
    fn slice_len(&self) -> &[u8] {
        self.slice()
    }
}
#[cfg(feature = "rpit")]
pub trait MemoryTr {
    fn slice(&self) -> &[u8];
    fn slice_len(&self) -> impl Deref<Target = [u8]> + '_ {
        self.slice()
    }
}
#[cfg(feature = "named")]
pub trait MemoryTr {
    type Slice<'a>: Deref<Target = [u8]>
    where
        Self: 'a;
    fn slice_len<'a>(&'a self) -> Self::Slice<'a>;
}

#[cfg(any(feature = "plain", feature = "rpit"))]
impl MemoryTr for Memory<'_> {
    fn slice(&self) -> &[u8] {
        self.bytes
    }
}
#[cfg(feature = "named")]
impl MemoryTr for Memory<'_> {
    type Slice<'a>
        = &'a [u8]
    where
        Self: 'a;
    fn slice_len<'a>(&'a self) -> Self::Slice<'a> {
        self.bytes
    }
}

pub fn size(memory: &Memory<'_>) -> usize {
    memory.slice_len().len()
}
