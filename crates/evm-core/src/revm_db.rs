//! どこで: Phase1のREVM DB実装 / 何を: StableStateへ接続 / なぜ: 実行エンジンと永続化を繋ぐため

use crate::selfdestruct::selfdestruct_address;
use evm_backend::stable_state::with_state_mut;
use evm_backend::types::keys::{make_account_key, make_code_key, make_storage_key};
use evm_backend::types::values::{AccountVal, CodeVal, U256Val};
use revm::database_interface::{Database, DatabaseCommit};
use revm::primitives::{Address, B256, StorageKey, StorageValue, U256, KECCAK_EMPTY};
use revm::state::{Account, AccountInfo, Bytecode};

#[derive(Clone, Copy, Debug)]
pub struct RevmStableDb;

impl Database for RevmStableDb {
    type Error = core::convert::Infallible;

    fn basic(&mut self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        let addr = address_to_bytes(address);
        let key = make_account_key(addr);
        let value = evm_backend::stable_state::with_state(|state| state.accounts.get(&key));
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
        let code = evm_backend::stable_state::with_state(|state| state.codes.get(&key));
        let bytecode = match code {
            Some(CodeVal(bytes)) => Bytecode::new_legacy(bytes.into()),
            None => Bytecode::default(),
        };
        Ok(bytecode)
    }

    fn storage(&mut self, address: Address, index: StorageKey) -> Result<StorageValue, Self::Error> {
        let addr = address_to_bytes(address);
        let slot = u256_to_bytes(index);
        let key = make_storage_key(addr, slot);
        let value = evm_backend::stable_state::with_state(|state| state.storage.get(&key));
        Ok(value.map(u256_val_to_u256).unwrap_or(U256::ZERO))
    }

    fn block_hash(&mut self, number: u64) -> Result<B256, Self::Error> {
        let hash = evm_backend::stable_state::with_state(|state| {
            state.blocks.get(&number).map(|b| b.block_hash)
        });
        Ok(B256::from(hash.unwrap_or([0u8; 32])))
    }
}

impl DatabaseCommit for RevmStableDb {
    fn commit(&mut self, changes: revm::primitives::HashMap<Address, Account>) {
        for (address, account) in changes.into_iter() {
            let addr = address_to_bytes(address);
            if account.is_selfdestructed() || (account.is_empty() && account.is_touched()) {
                selfdestruct_address(addr);
                continue;
            }

            let info = account.info.clone();
            let key = make_account_key(addr);
            let val = info_to_account_val(&info);

            with_state_mut(|state| {
                state.accounts.insert(key, val);

                for (slot, entry) in account.changed_storage_slots() {
                    let storage_key = make_storage_key(addr, u256_to_bytes(*slot));
                    let present = entry.present_value;
                    if present.is_zero() {
                        state.storage.remove(&storage_key);
                    } else {
                        state.storage.insert(storage_key, U256Val(u256_to_bytes(present)));
                    }
                }

                if let Some(code) = info.code.clone() {
                    let code_hash = b256_to_bytes(info.code_hash);
                    let code_key = make_code_key(code_hash);
                    let bytes = code.original_byte_slice().to_vec();
                    if bytes.is_empty() {
                        state.codes.remove(&code_key);
                    } else {
                        state.codes.insert(code_key, CodeVal(bytes));
                    }
                }
            });
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

fn address_to_bytes(address: Address) -> [u8; 20] {
    let mut out = [0u8; 20];
    out.copy_from_slice(address.as_ref());
    out
}

fn b256_to_bytes(hash: B256) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(hash.as_ref());
    out
}

fn u256_to_bytes(value: U256) -> [u8; 32] {
    value.to_be_bytes()
}

fn u256_val_to_u256(value: U256Val) -> U256 {
    U256::from_be_bytes(value.0)
}
