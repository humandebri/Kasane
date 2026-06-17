//! どこで: Phase1のREVM DB実装 / 何を: StableStateへ接続 / なぜ: 実行エンジンと永続化を繋ぐため

use crate::bytes::{b256_to_bytes, try_address_to_bytes, u256_to_bytes};
use crate::selfdestruct::selfdestruct_address;
use evm_db::stable_state::{bump_evm_state_epoch, with_state_mut};
use evm_db::types::keys::{make_account_key, make_code_key, make_storage_key};
use evm_db::types::values::{AccountVal, CodeVal, U256Val};
use evm_db::Storable;
use revm::database_interface::{Database, DatabaseCommit, DatabaseRef};
use revm::primitives::{Address, StorageKey, StorageValue, B256, KECCAK_EMPTY, U256};
use revm::state::{Account, AccountInfo, Bytecode};
use std::borrow::Cow;
use verified_core::state_diff::{
    account_commit_decision, code_commit_decision, storage_commit_decision, AccountCommitDecision,
    CodeCommitDecision, StorageCommitDecision,
};

#[derive(Clone, Copy, Debug)]
pub struct RevmStableDb;

impl Database for RevmStableDb {
    type Error = core::convert::Infallible;

    fn basic(&mut self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        let addr = try_address_to_bytes(address).expect("revm address must be 20 bytes");
        let key = make_account_key(addr);
        let value = evm_db::stable_state::with_state(|state| state.accounts.get(&key));
        let info = match value {
            Some(account) => account_val_to_info(&account),
            None => return Ok(None),
        };
        Ok(Some(info))
    }

    fn code_by_hash(&mut self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        if code_hash == KECCAK_EMPTY {
            return Ok(Bytecode::default());
        }
        let key = make_code_key(b256_to_bytes(code_hash));
        let code = evm_db::stable_state::with_state(|state| state.codes.get(&key));
        let bytecode = match code {
            Some(CodeVal(bytes)) => Bytecode::new_legacy(bytes.into()),
            None => Bytecode::default(),
        };
        Ok(bytecode)
    }

    fn storage(
        &mut self,
        address: Address,
        index: StorageKey,
    ) -> Result<StorageValue, Self::Error> {
        let addr = try_address_to_bytes(address).expect("revm address must be 20 bytes");
        let slot = u256_to_bytes(index);
        let key = make_storage_key(addr, slot);
        let value = evm_db::stable_state::with_state(|state| state.storage.get(&key));
        Ok(value.map(u256_val_to_u256).unwrap_or(U256::ZERO))
    }

    fn block_hash(&mut self, number: u64) -> Result<B256, Self::Error> {
        let hash = evm_db::stable_state::with_state(|state| {
            if let Some(ptr) = state.blocks.get(&number) {
                let bytes = state.blob_store.read(&ptr).ok()?;
                let block = evm_db::chain_data::BlockData::from_bytes(Cow::Owned(bytes));
                return Some(block.block_hash);
            }
            None
        });
        Ok(B256::from(hash.unwrap_or([0u8; 32])))
    }
}

impl DatabaseRef for RevmStableDb {
    type Error = core::convert::Infallible;

    fn basic_ref(&self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        let addr = try_address_to_bytes(address).expect("revm address must be 20 bytes");
        let key = make_account_key(addr);
        let value = evm_db::stable_state::with_state(|state| state.accounts.get(&key));
        let info = match value {
            Some(account) => account_val_to_info(&account),
            None => return Ok(None),
        };
        Ok(Some(info))
    }

    fn code_by_hash_ref(&self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        if code_hash == KECCAK_EMPTY {
            return Ok(Bytecode::default());
        }
        let key = make_code_key(b256_to_bytes(code_hash));
        let code = evm_db::stable_state::with_state(|state| state.codes.get(&key));
        let bytecode = match code {
            Some(CodeVal(bytes)) => Bytecode::new_legacy(bytes.into()),
            None => Bytecode::default(),
        };
        Ok(bytecode)
    }

    fn storage_ref(
        &self,
        address: Address,
        index: StorageKey,
    ) -> Result<StorageValue, Self::Error> {
        let addr = try_address_to_bytes(address).expect("revm address must be 20 bytes");
        let slot = u256_to_bytes(index);
        let key = make_storage_key(addr, slot);
        let value = evm_db::stable_state::with_state(|state| state.storage.get(&key));
        Ok(value.map(u256_val_to_u256).unwrap_or(U256::ZERO))
    }

    fn block_hash_ref(&self, number: u64) -> Result<B256, Self::Error> {
        let hash = evm_db::stable_state::with_state(|state| {
            if let Some(ptr) = state.blocks.get(&number) {
                let bytes = state.blob_store.read(&ptr).ok()?;
                let block = evm_db::chain_data::BlockData::from_bytes(Cow::Owned(bytes));
                return Some(block.block_hash);
            }
            None
        });
        Ok(B256::from(hash.unwrap_or([0u8; 32])))
    }
}

impl DatabaseCommit for RevmStableDb {
    fn commit(&mut self, changes: revm::primitives::HashMap<Address, Account>) {
        let mut mutated = false;
        for (address, account) in changes.into_iter() {
            let addr = try_address_to_bytes(address).expect("revm address must be 20 bytes");
            match account_commit_decision(
                account.is_selfdestructed(),
                account.is_empty(),
                account.is_touched(),
            ) {
                AccountCommitDecision::Delete => {
                    selfdestruct_address(addr);
                    mutated = true;
                    continue;
                }
                AccountCommitDecision::Upsert => {}
            }

            let info = account.info.clone();
            let key = make_account_key(addr);
            let val = info_to_account_val(&info);

            with_state_mut(|state| {
                state.accounts.insert(key, val);
                mutated = true;

                for (slot, entry) in account.changed_storage_slots() {
                    let storage_key = make_storage_key(addr, u256_to_bytes(*slot));
                    let present = entry.present_value;
                    match storage_commit_decision(present.is_zero()) {
                        StorageCommitDecision::Remove => {
                            state.storage.remove(&storage_key);
                        }
                        StorageCommitDecision::Insert => {
                            state
                                .storage
                                .insert(storage_key, U256Val(u256_to_bytes(present)));
                        }
                    }
                }

                let code = info.code.clone();
                let code_decision = code_commit_decision(
                    code.is_some(),
                    code.as_ref()
                        .is_some_and(|value| value.original_byte_slice().is_empty()),
                );
                match code_decision {
                    CodeCommitDecision::Skip => {}
                    CodeCommitDecision::Remove | CodeCommitDecision::Insert => {
                        let code = code.expect("code must exist when decision is not skip");
                        let code_hash = b256_to_bytes(info.code_hash);
                        let code_key = make_code_key(code_hash);
                        let bytes = code.original_byte_slice().to_vec();
                        match code_decision {
                            CodeCommitDecision::Skip => {}
                            CodeCommitDecision::Remove => {
                                state.codes.remove(&code_key);
                            }
                            CodeCommitDecision::Insert => {
                                state.codes.insert(code_key, CodeVal(bytes));
                            }
                        }
                    }
                }
            });
        }
        if mutated {
            bump_evm_state_epoch();
        }
    }
}

fn account_val_to_info(val: &AccountVal) -> AccountInfo {
    let balance = U256::from_be_bytes(val.balance());
    let code_hash = B256::from(val.code_hash());
    AccountInfo {
        balance,
        nonce: val.nonce(),
        code_hash,
        account_id: None,
        code: None,
    }
}

fn info_to_account_val(info: &AccountInfo) -> AccountVal {
    let balance = info.balance.to_be_bytes();
    let code_hash = b256_to_bytes(info.code_hash);
    AccountVal::from_parts(info.nonce, balance, code_hash)
}

fn u256_val_to_u256(value: U256Val) -> U256 {
    U256::from_be_bytes(value.0)
}
