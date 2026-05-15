# edge-case review

No blocking issue.

`block_gas_limit == 0` intentionally disables the gas bound, matching `verified_core::block::tx_fits_block_gas`.
