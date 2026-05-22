//! どこで: wrap quote/native金額PBT / 何を: 承認上限・fee構成・net金額 / なぜ: 過大徴収とunderflowを検出するため

use proptest::prelude::*;
use verified_core::native_amount::{
    native_withdraw_amount_safe_raw, native_withdraw_receive_amount,
};
use verified_core::wrap_quote::{
    wrap_quote_approval_safe_raw, wrap_quote_components_safe_raw, GAS_PRICE_DENOMINATOR_BPS,
    WEI_PER_E8S,
};
use verified_core::wrap_request::{native_deposit_retry_allowed_raw, WRAP_REQUEST_STATUS_RUNNING};

proptest! {
    #[test]
    fn pbt_native_deposit_retry_and_withdraw_amount_are_exact(
        gas_limit_zero in 0u64..3,
        status in 0u64..5,
        mint_failed_recoverable in 0u64..3,
        pull_ledger_tx_id_present in 0u64..3,
        amount_e8s in any::<u128>(),
        ledger_fee_e8s in any::<u128>(),
        receive_present in 0u64..3,
        receive_e8s in any::<u128>(),
    ) {
        prop_assert_eq!(
            native_deposit_retry_allowed_raw(
                gas_limit_zero,
                status,
                mint_failed_recoverable,
                pull_ledger_tx_id_present,
            ),
            gas_limit_zero == 1
                && mint_failed_recoverable == 1
                && status != WRAP_REQUEST_STATUS_RUNNING
                && pull_ledger_tx_id_present == 1
        );
        prop_assert_eq!(
            native_withdraw_receive_amount(amount_e8s, ledger_fee_e8s),
            amount_e8s.checked_sub(ledger_fee_e8s)
        );
        prop_assert_eq!(
            native_withdraw_amount_safe_raw(
                amount_e8s,
                ledger_fee_e8s,
                receive_present,
                receive_e8s,
            ),
            (amount_e8s >= ledger_fee_e8s
                && receive_present == 1
                && receive_e8s == amount_e8s - ledger_fee_e8s)
                || (amount_e8s < ledger_fee_e8s && receive_present == 0)
        );
    }

    #[test]
    fn pbt_wrap_quote_approval_and_components_are_exact(
        ledger_matches in 0u64..3,
        charged_fee_e8s in any::<u128>(),
        max_fee_e8s in any::<u128>(),
        charged_gas_price_wei in any::<u128>(),
        quoted_gas_price_wei in any::<u128>(),
        base_gas_price_wei in any::<u128>(),
        gas_price_buffer_bps in any::<u64>(),
        gas_limit in any::<u64>(),
        cycle_fee_e8s in any::<u64>(),
        gas_fee_e8s in any::<u128>(),
    ) {
        prop_assert_eq!(
            wrap_quote_approval_safe_raw(
                ledger_matches,
                charged_fee_e8s,
                max_fee_e8s,
                charged_gas_price_wei,
                quoted_gas_price_wei,
            ),
            ledger_matches == 1
                && charged_fee_e8s <= max_fee_e8s
                && charged_gas_price_wei <= quoted_gas_price_wei
        );

        let expected_gas_price = base_gas_price_wei
            .saturating_mul(u128::from(gas_price_buffer_bps))
            .saturating_add(GAS_PRICE_DENOMINATOR_BPS - 1)
            / GAS_PRICE_DENOMINATOR_BPS;
        let expected_gas_fee = charged_gas_price_wei
            .saturating_mul(u128::from(gas_limit))
            .saturating_add(WEI_PER_E8S - 1)
            / WEI_PER_E8S;
        let expected_charged_fee = gas_fee_e8s.saturating_add(u128::from(cycle_fee_e8s));
        prop_assert_eq!(
            wrap_quote_components_safe_raw(
                u64::from(charged_gas_price_wei == expected_gas_price),
                u64::from(gas_fee_e8s == expected_gas_fee),
                u64::from(charged_fee_e8s == expected_charged_fee),
            ),
            charged_gas_price_wei == expected_gas_price
                && gas_fee_e8s == expected_gas_fee
                && charged_fee_e8s == expected_charged_fee
        );
    }
}
