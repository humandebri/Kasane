Generate scenario candidates:
pub fn prune_boundary_safe(previous_present: bool, previous: u64, next_present: bool, next_boundary: u64, head: u64, retain: u64) -> bool
{
    if !next_present {
        return true;
    }
    if retain == 0 || head <= retain || next_boundary > head - retain {
        return false;
    }
    if previous_present {
        previous <= next_boundary
    } else {
        true
    }
}
