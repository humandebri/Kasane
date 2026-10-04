#[cfg(feature = "dependent")]
pub mod dependent {
    use std::ops::Deref;
    pub struct Memory<'a, T> {
        pub bytes: &'a [T],
    }
    pub trait MemoryTr<T> {
        type View<'a>: Deref<Target = [T]>
        where
            Self: 'a,
            T: 'a;
        fn view<'a>(&'a self) -> Self::View<'a>;
    }
    impl<T> MemoryTr<T> for Memory<'_, T> {
        type View<'a>
            = &'a [T]
        where
            Self: 'a,
            T: 'a;
        fn view<'a>(&'a self) -> Self::View<'a> {
            self.bytes
        }
    }
    pub fn size<T>(memory: &Memory<'_, T>) -> usize {
        memory.view().len()
    }
}
#[cfg(feature = "mixed")]
pub mod mixed {
    use std::ops::Deref;
    pub struct Memory<'a> {
        pub bytes: &'a [u8],
        pub words: &'a [u16],
    }
    pub trait MemoryTr {
        type Bytes<'a>: Deref<Target = [u8]>
        where
            Self: 'a;
        type Words<'a>: Deref<Target = [u16]>
        where
            Self: 'a;
        fn bytes<'a>(&'a self) -> Self::Bytes<'a>;
        fn words<'a>(&'a self) -> Self::Words<'a>;
    }
    impl MemoryTr for Memory<'_> {
        type Bytes<'a>
            = &'a [u8]
        where
            Self: 'a;
        type Words<'a>
            = &'a [u16]
        where
            Self: 'a;
        fn bytes<'a>(&'a self) -> Self::Bytes<'a> {
            self.bytes
        }
        fn words<'a>(&'a self) -> Self::Words<'a> {
            self.words
        }
    }
    pub fn byte_size(memory: &Memory<'_>) -> usize {
        memory.bytes().len()
    }
    pub fn word_size(memory: &Memory<'_>) -> usize {
        memory.words().len()
    }
}
