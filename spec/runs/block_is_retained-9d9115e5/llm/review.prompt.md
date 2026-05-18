Review as implementation, edge-case, adversarial, Verus:
pub fn block_is_retained(head: u64, retain: u64, block: u64) -> bool
{
    if block > head {
        return false;
    }
    if retain == 0 || head <= retain {
        return true;
    }
    block > head - retain
}
