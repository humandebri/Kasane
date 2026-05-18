Review as implementation, edge-case, adversarial, Verus:
pub fn block_is_prunable(head: u64, retain: u64, block: u64) -> bool
{
    if retain == 0 || head <= retain {
        return false;
    }
    block <= head - retain
}
