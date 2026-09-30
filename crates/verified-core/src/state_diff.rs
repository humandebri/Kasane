//! どこで: revm state diff適用 / 何を: account/storage/codeの削除・更新判定 / なぜ: revm境界のadapterから状態遷移分岐を分離するため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccountCommitDecision {
    Skip,
    Delete,
    Upsert,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageCommitDecision {
    Remove,
    Insert,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodeCommitDecision {
    Skip,
    Remove,
    Insert,
}

#[cfg_attr(verus_keep_ghost, verus_spec(empty => ensures
    empty == (nonce == 0 && balance_is_zero && code_is_empty),
))]
pub fn account_is_empty(nonce: u64, balance_is_zero: bool, code_is_empty: bool) -> bool {
    nonce == 0 && balance_is_zero && code_is_empty
}

#[cfg_attr(verus_keep_ghost, verus_spec(decision => ensures
    !is_touched ==> decision == AccountCommitDecision::Skip,
    is_touched && (is_selfdestructed || is_empty)
        ==> decision == AccountCommitDecision::Delete,
    is_touched && !is_selfdestructed && !is_empty
        ==> decision == AccountCommitDecision::Upsert,
))]
pub fn account_commit_decision(
    is_selfdestructed: bool,
    is_empty: bool,
    is_touched: bool,
) -> AccountCommitDecision {
    if !is_touched {
        AccountCommitDecision::Skip
    } else if is_selfdestructed || is_empty {
        AccountCommitDecision::Delete
    } else {
        AccountCommitDecision::Upsert
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(decision => ensures
    is_zero ==> decision == StorageCommitDecision::Remove,
    !is_zero ==> decision == StorageCommitDecision::Insert,
))]
pub fn storage_commit_decision(is_zero: bool) -> StorageCommitDecision {
    if is_zero {
        StorageCommitDecision::Remove
    } else {
        StorageCommitDecision::Insert
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(decision => ensures
    !has_code ==> decision == CodeCommitDecision::Skip,
    has_code && code_is_empty ==> decision == CodeCommitDecision::Remove,
    has_code && !code_is_empty ==> decision == CodeCommitDecision::Insert,
))]
pub fn code_commit_decision(has_code: bool, code_is_empty: bool) -> CodeCommitDecision {
    if !has_code {
        CodeCommitDecision::Skip
    } else if code_is_empty {
        CodeCommitDecision::Remove
    } else {
        CodeCommitDecision::Insert
    }
}

#[cfg(test)]
mod tests {
    use super::{
        account_commit_decision, account_is_empty, code_commit_decision, storage_commit_decision,
        AccountCommitDecision, CodeCommitDecision, StorageCommitDecision,
    };

    #[test]
    fn account_commit_skips_untouched_and_deletes_empty_touched_accounts() {
        assert_eq!(
            account_commit_decision(true, false, true),
            AccountCommitDecision::Delete
        );
        assert_eq!(
            account_commit_decision(false, true, true),
            AccountCommitDecision::Delete
        );
        assert_eq!(
            account_commit_decision(false, true, false),
            AccountCommitDecision::Skip
        );
        assert_eq!(
            account_commit_decision(true, false, false),
            AccountCommitDecision::Skip
        );
        assert_eq!(
            account_commit_decision(false, false, false),
            AccountCommitDecision::Skip
        );
        assert_eq!(
            account_commit_decision(false, false, true),
            AccountCommitDecision::Upsert
        );
    }

    #[test]
    fn empty_account_requires_zero_nonce_balance_and_code() {
        assert!(account_is_empty(0, true, true));
        assert!(!account_is_empty(1, true, true));
        assert!(!account_is_empty(0, false, true));
        assert!(!account_is_empty(0, true, false));
    }

    #[test]
    fn storage_and_code_commit_match_presence() {
        assert_eq!(storage_commit_decision(true), StorageCommitDecision::Remove);
        assert_eq!(
            storage_commit_decision(false),
            StorageCommitDecision::Insert
        );
        assert_eq!(code_commit_decision(false, false), CodeCommitDecision::Skip);
        assert_eq!(code_commit_decision(true, true), CodeCommitDecision::Remove);
        assert_eq!(
            code_commit_decision(true, false),
            CodeCommitDecision::Insert
        );
    }
}
