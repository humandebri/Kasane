// Compile the actual pure production functions, without a copied Rust model.
#[allow(dead_code)]
#[path = "../../crates/verified-core/src/fee.rs"]
mod fee;
#[allow(dead_code)]
#[path = "../../crates/verified-core/src/state_diff.rs"]
mod state_diff;

fn main() {
    let m64 = u128::from(u64::MAX);
    let values = [0, 1, 7, m64 - 1, m64, m64 + 1, u128::MAX - 1, u128::MAX];
    for cap in values {
        for priority in values {
            for base in [0, 1, 7, u64::MAX] {
                let result = fee::effective_gas_price(cap, priority, base)
                    .map_or_else(|| "none".to_string(), |price| price.to_string());
                println!("price {cap} {priority} {base} {result}");
            }
        }
    }
    for gas in [0, 1, 21000, u64::MAX] {
        for price in [0, 1, 7, u64::MAX] {
            for l1 in [0, 1, u128::MAX] {
                for operator in [0, 1, u128::MAX] {
                    println!(
                        "fee {gas} {price} {l1} {operator} {}",
                        fee::total_fee(gas, price, l1, operator)
                    );
                }
            }
            println!("reward {gas} {price} {}", fee::base_fee_reward(gas, price));
        }
    }
    for destroyed in [false, true] {
        for empty in [false, true] {
            for touched in [false, true] {
                let delete = state_diff::account_commit_decision(destroyed, empty, touched)
                    == state_diff::AccountCommitDecision::Delete;
                println!("account {destroyed} {empty} {touched} {delete}");
            }
        }
    }
    for has_code in [false, true] {
        for empty in [false, true] {
            let decision = match state_diff::code_commit_decision(has_code, empty) {
                state_diff::CodeCommitDecision::Skip => "skip",
                state_diff::CodeCommitDecision::Remove => "remove",
                state_diff::CodeCommitDecision::Insert => "insert",
            };
            println!("code {has_code} {empty} {decision}");
        }
    }
    // storage_commit_decision accepts the result of U256::is_zero, not a U256.
    for (value, zero) in [
        ("0", true),
        ("1", false),
        (
            "115792089237316195423570985008687907853269984665640564039457584007913129639935",
            false,
        ),
    ] {
        let decision = match state_diff::storage_commit_decision(zero) {
            state_diff::StorageCommitDecision::Remove => "remove",
            state_diff::StorageCommitDecision::Insert => "insert",
        };
        println!("storage {value} {decision}");
    }
}
