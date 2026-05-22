Review as implementation, edge-case, adversarial, Verus:
pub fn staged_tx_is_current_pending_raw(
    ready_points_to_tx: u64,
    pending_meta_points_to_tx: u64,
    current_pending_points_to_tx: u64,
    tx_payload_present: u64,
    tx_not_marked_dropped: u64,
) -> bool
{
    ready_points_to_tx == 1
        && pending_meta_points_to_tx == 1
        && current_pending_points_to_tx == 1
        && tx_payload_present == 1
        && tx_not_marked_dropped == 1
}
