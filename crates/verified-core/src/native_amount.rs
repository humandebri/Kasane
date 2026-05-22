//! どこで: native withdraw 金額境界 / 何を: grossからledger fee控除後のnet / なぜ: underflowと過払いを防ぐため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_spec(receive => ensures
    amount_e8s < ledger_fee_e8s ==> receive == Option::<u128>::None,
    amount_e8s >= ledger_fee_e8s ==> receive == Option::<u128>::Some((amount_e8s - ledger_fee_e8s) as u128),
))]
pub fn native_withdraw_receive_amount(amount_e8s: u128, ledger_fee_e8s: u128) -> Option<u128> {
    if amount_e8s < ledger_fee_e8s {
        return None;
    }
    Some(amount_e8s - ledger_fee_e8s)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    result == (
        (amount_e8s >= ledger_fee_e8s
            && receive_present == 1
            && receive_e8s == (amount_e8s - ledger_fee_e8s) as u128)
        || (amount_e8s < ledger_fee_e8s
            && receive_present == 0)
    ),
))]
pub fn native_withdraw_amount_safe_raw(
    amount_e8s: u128,
    ledger_fee_e8s: u128,
    receive_present: u64,
    receive_e8s: u128,
) -> bool {
    (amount_e8s >= ledger_fee_e8s
        && receive_present == 1
        && receive_e8s == amount_e8s - ledger_fee_e8s)
        || (amount_e8s < ledger_fee_e8s && receive_present == 0)
}
