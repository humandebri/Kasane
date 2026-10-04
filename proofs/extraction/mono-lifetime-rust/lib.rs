// Lifetime-only frontend probe; not an EVM or memory-provenance model.
pub struct Same<'a> {
    pub left: &'a mut u32,
    pub right: &'a mut u32,
}

pub struct Split<'a, 'b> {
    pub left: &'a mut u32,
    pub right: &'b mut u32,
}

pub fn same_noop(_context: Same<'_>) {}

pub fn split_noop(_context: Split<'_, '_>) {}

pub fn split_left<'a, 'b>(context: Split<'a, 'b>) -> &'a mut u32 {
    context.left
}

pub fn split_right<'a, 'b>(context: Split<'a, 'b>) -> &'b mut u32 {
    context.right
}
