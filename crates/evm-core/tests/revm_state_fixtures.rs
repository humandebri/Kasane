//! Execute pinned Ethereum Prague fixtures against the vendored engine, without
//! Kasane's intentional base-fee credit or custom precompiles.
use alloy_trie::{
    root::{state_root_unhashed, storage_root_unhashed},
    TrieAccount,
};
use evm_core::revm_exec::EVM_SPEC_ID;
use revm::{
    context::{Context, TxEnv},
    database::InMemoryDB,
    primitives::{keccak256, Address, Bytes, B256, U256},
    state::{AccountInfo, Bytecode},
    ExecuteCommitEvm, MainBuilder, MainContext,
};
use serde_json::Value;

fn quantity(v: &Value) -> U256 {
    v.as_str().unwrap().parse().unwrap()
}
fn bytes(v: &Value) -> Bytes {
    v.as_str().unwrap().parse().unwrap()
}
fn address(v: &Value) -> Address {
    v.as_str().unwrap().parse().unwrap()
}

#[test]
fn official_prague_revert_state_tests() {
    assert_eq!(EVM_SPEC_ID, revm::primitives::hardfork::SpecId::PRAGUE);
    let fixtures = [
        include_str!("../../../proofs/evm/fixtures/ethereum/RevertInCallCode.json"),
        include_str!("../../../proofs/evm/fixtures/ethereum/RevertInDelegateCall.json"),
        include_str!("../../../proofs/evm/fixtures/ethereum/RevertSubCallStorageOOG.json"),
        include_str!("../../../proofs/evm/fixtures/ethereum/RevertOpcodeMultipleSubCalls.json"),
    ];
    let mut checked = 0;
    for fixture in fixtures {
        let tests: Value = serde_json::from_str(fixture).unwrap();
        for (name, test) in tests.as_object().unwrap() {
            let env = &test["env"];
            let transaction = &test["transaction"];
            // This runner deliberately supports only the legacy CALL transactions
            // in these four fixed fixtures; new fixture types need an explicit review.
            assert!(transaction.get("gasPrice").is_some());
            assert!(transaction.get("accessLists").is_none());
            assert!(transaction.get("authorizationList").is_none());
            for post in test["post"]["Prague"].as_array().expect("Prague fixture") {
                assert!(post.get("expectException").is_none());
                let mut db = InMemoryDB::default();
                for (addr, pre) in test["pre"].as_object().unwrap() {
                    let addr: Address = addr.parse().unwrap();
                    let code = Bytecode::new_raw(bytes(&pre["code"]));
                    db.insert_account_info(
                        addr,
                        AccountInfo {
                            balance: quantity(&pre["balance"]),
                            nonce: quantity(&pre["nonce"]).to(),
                            code_hash: code.hash_slow(),
                            code: Some(code),
                            ..Default::default()
                        },
                    );
                    for (key, value) in pre["storage"].as_object().unwrap() {
                        db.insert_account_storage(addr, key.parse().unwrap(), quantity(value))
                            .unwrap();
                    }
                }
                let index = |field: &str| post["indexes"][field].as_u64().unwrap() as usize;
                let tx = TxEnv::builder()
                    .caller(address(&transaction["sender"]))
                    .kind(revm::primitives::TxKind::Call(address(&transaction["to"])))
                    .nonce(quantity(&transaction["nonce"]).to())
                    .gas_limit(quantity(&transaction["gasLimit"][index("gas")]).to())
                    .gas_price(quantity(&transaction["gasPrice"]).to())
                    .value(quantity(&transaction["value"][index("value")]))
                    .data(bytes(&transaction["data"][index("data")]))
                    .build()
                    .unwrap();
                let mut evm = Context::mainnet()
                    .with_db(&mut db)
                    .modify_cfg_chained(|cfg| cfg.set_spec_and_mainnet_gas_params(EVM_SPEC_ID))
                    .modify_block_chained(|block| {
                        block.number = quantity(&env["currentNumber"]);
                        block.timestamp = quantity(&env["currentTimestamp"]);
                        block.gas_limit = quantity(&env["currentGasLimit"]).to();
                        block.basefee = quantity(&env["currentBaseFee"]).to();
                        block.beneficiary = address(&env["currentCoinbase"]);
                        block.difficulty = quantity(&env["currentDifficulty"]);
                        block.prevrandao =
                            Some(env["currentRandom"].as_str().unwrap().parse().unwrap());
                        block.set_blob_excess_gas_and_price(
                            quantity(&env["currentExcessBlobGas"]).to(),
                            revm::primitives::eip4844::BLOB_BASE_FEE_UPDATE_FRACTION_PRAGUE,
                        );
                    })
                    .build_mainnet();
                let result = evm.transact_commit(tx).expect(name);
                drop(evm);
                let root =
                    state_root_unhashed(db.cache.accounts.iter().filter_map(|(addr, account)| {
                        // The fixed post-EIP-161 fixtures contain no pre-existing
                        // empty accounts to preserve. Omit empty coinbase cache entries.
                        account.info().filter(|info| !info.is_empty()).map(|info| {
                            (
                                *addr,
                                TrieAccount {
                                    nonce: info.nonce,
                                    balance: info.balance,
                                    code_hash: info.code_hash,
                                    storage_root: storage_root_unhashed(
                                        account
                                            .storage
                                            .iter()
                                            .filter(|(_, value)| !value.is_zero())
                                            .map(|(key, value)| {
                                                (B256::from(key.to_be_bytes::<32>()), *value)
                                            }),
                                    ),
                                },
                            )
                        })
                    }));
                assert_eq!(
                    root,
                    post["hash"].as_str().unwrap().parse::<B256>().unwrap(),
                    "{name}: {:?}",
                    post["indexes"]
                );
                let logs_hash = keccak256(alloy_rlp::encode(result.logs().to_vec()));
                assert_eq!(
                    logs_hash,
                    post["logs"].as_str().unwrap().parse::<B256>().unwrap(),
                    "{name}"
                );
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 38, "fixture coverage changed");
}
