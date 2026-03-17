//! どこで: wrap / unwrap の PocketIC E2E
//! 何を: real wrap canister と gateway 間の経路を運用に近い形で固定
//! なぜ: gas price の canister 間呼び出し種別と unwrap dispatch の実挙動を回帰から守るため

use candid::{CandidType, Decode, Deserialize, Encode, Nat, Principal};
use evm_core::hash;
use evm_core::wrap_precompile::WRAP_PRECOMPILE_ADDRESS;
use pocket_ic::PocketIc;
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;
use tiny_keccak::{Hasher, Keccak};

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct GenesisBalanceView {
    address: Vec<u8>,
    amount: u128,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct GatewayInitArgs {
    genesis_balances: Vec<GenesisBalanceView>,
    wrap_canister_id: Principal,
    wrap_factory_address: Vec<u8>,
    query_instruction_soft_limit: Option<u64>,
    update_instruction_soft_limit: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct WrapInitArgs {
    kasane_canister: Principal,
    evm_gateway_canister: Principal,
    fee_ledger_canister: Principal,
    wrap_factory_address: Vec<u8>,
    cycle_fee_e8s: u64,
    gas_price_buffer_bps: u32,
    allowed_assets: Vec<Principal>,
}

const WRAP_AMOUNT_E8S: u128 = 1_000_000_000_000u128;
const TEST_ASSET_DECIMALS: u8 = 8;
const TEST_LEDGER_BALANCE: u128 = 10_000_000_000_000u128;
const TEST_GENESIS_BALANCE_WEI: u128 = 10_000_000_000_000_000_000_000_000u128;
const TEST_CHAIN_ID: u64 = 4_801_360;
const TEST_WRAP_GAS_LIMIT: u64 = 3_000_000;

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct SubmitIcTxArgsDto {
    to: Option<Vec<u8>>,
    from: Option<Vec<u8>>,
    value: Nat,
    max_priority_fee_per_gas: Nat,
    data: Vec<u8>,
    max_fee_per_gas: Nat,
    nonce: u64,
    gas_limit: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct SubmitWrapRequestArgs {
    asset_id: Principal,
    amount_e8s: Nat,
    evm_recipient: Vec<u8>,
    evm_nonce: u64,
    gas_limit: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct SubmitWrapRequestOk {
    request_id: Vec<u8>,
    charged_fee_e8s: Nat,
    charged_gas_price_wei: Nat,
    fee_ledger_tx_id: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum SubmitTxError {
    InvalidArgument(String),
    Rejected(String),
    Internal(String),
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum PendingStatusView {
    Queued { seq: u64 },
    Included { block_number: u64, tx_index: u32 },
    Dropped { code: u16 },
    Unknown,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct ApiErrorDetail {
    code: String,
    message: String,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum ApiError {
    InvalidArgument(ApiErrorDetail),
    Rejected(ApiErrorDetail),
    Internal(ApiErrorDetail),
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum RequestDispatchStatusView {
    Queued,
    Dispatching,
    Dispatched,
    DispatchFailed,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct UnwrapDispatchOverviewView {
    request_id: Vec<u8>,
    status: RequestDispatchStatusView,
    error: Option<String>,
}

#[derive(Clone, Copy, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum RequestKind {
    Wrap,
    Unwrap,
}

#[derive(Clone, Copy, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum WrapRequestStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct RequestOverview {
    kind: RequestKind,
    request_id: Vec<u8>,
    status: WrapRequestStatus,
    error: Option<ApiErrorDetail>,
    fee_ledger_tx_id: Option<Vec<u8>>,
    pull_ledger_tx_id: Option<Vec<u8>>,
    mint_tx_id: Option<Vec<u8>>,
    withdraw_ledger_tx_id: Option<Vec<u8>>,
    ledger_tx_id: Option<Vec<u8>>,
    dispatch_status: Option<RequestDispatchStatusView>,
    dispatch_error: Option<String>,
}

#[derive(Clone, Copy, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum UnwrapReadiness {
    Ready,
    TokenNotDeployed,
    InsufficientBalance,
    InsufficientAllowance,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct GetUnwrapRequirementsArgs {
    asset_id: Principal,
    amount_e8s: Nat,
    caller_evm_address: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct GetUnwrapRequirementsOk {
    factory_address: Vec<u8>,
    wrapped_token_address: Option<Vec<u8>>,
    balance: Nat,
    allowance: Nat,
    approve_required: bool,
    readiness: UnwrapReadiness,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct DispatchUnwrapRequestArgs {
    request_id: Vec<u8>,
    asset_id: Principal,
    amount_e8s: Nat,
    recipient: Principal,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct DispatchUnwrapRequestOk {
    request_id: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct RetryRequestArgs {
    request_id: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct RecoverFailedWrapArgs {
    request_id: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct ReceiptView {
    tx_id: Vec<u8>,
    block_number: u64,
    tx_index: u32,
    status: u8,
    gas_used: u64,
    effective_gas_price: u64,
    l1_data_fee: u128,
    operator_fee: u128,
    total_fee: u128,
    return_data_hash: Vec<u8>,
    return_data: Option<Vec<u8>>,
    contract_address: Option<Vec<u8>>,
    logs: Vec<LogView>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct LogView {
    address: Vec<u8>,
    topics: Vec<Vec<u8>>,
    data: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum LookupError {
    NotFound,
    Pruned { pruned_before_block: u64 },
    Pending,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq, Ord, PartialOrd)]
struct LedgerAccount {
    owner: Principal,
    subaccount: Option<Vec<u8>>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct FeatureFlags {
    icrc2: bool,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum MetadataValue {
    Nat(Nat),
    Int(candid::Int),
    Text(String),
    Blob(Vec<u8>),
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct ArchiveOptions {
    num_blocks_to_archive: u64,
    max_transactions_per_response: Option<u64>,
    trigger_threshold: u64,
    max_message_size_bytes: Option<u64>,
    cycles_for_archive_creation: Option<u64>,
    node_max_memory_size_bytes: Option<u64>,
    controller_id: Principal,
    more_controller_ids: Option<Vec<Principal>>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct LedgerInitArgs {
    minting_account: LedgerAccount,
    fee_collector_account: Option<LedgerAccount>,
    transfer_fee: Nat,
    decimals: Option<u8>,
    max_memo_length: Option<u16>,
    token_symbol: String,
    token_name: String,
    metadata: Vec<(String, MetadataValue)>,
    initial_balances: Vec<(LedgerAccount, Nat)>,
    feature_flags: Option<FeatureFlags>,
    archive_options: ArchiveOptions,
    index_principal: Option<Principal>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum LedgerArg {
    Init(LedgerInitArgs),
    Upgrade(Option<()>),
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct LedgerApproveArgs {
    fee: Option<Nat>,
    memo: Option<Vec<u8>>,
    from_subaccount: Option<Vec<u8>>,
    created_at_time: Option<u64>,
    amount: Nat,
    expected_allowance: Option<Nat>,
    expires_at: Option<u64>,
    spender: LedgerAccount,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum LedgerApproveError {
    GenericError { message: String, error_code: Nat },
    TemporarilyUnavailable,
    Duplicate { duplicate_of: Nat },
    BadFee { expected_fee: Nat },
    AllowanceChanged { current_allowance: Nat },
    CreatedInFuture { ledger_time: u64 },
    TooOld,
    Expired { ledger_time: u64 },
    InsufficientFunds { balance: Nat },
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct RpcCallObjectView {
    to: Option<Vec<u8>>,
    from: Option<Vec<u8>>,
    gas: Option<u64>,
    gas_price: Option<u128>,
    nonce: Option<u64>,
    max_fee_per_gas: Option<u128>,
    max_priority_fee_per_gas: Option<u128>,
    chain_id: Option<u64>,
    tx_type: Option<u64>,
    access_list: Option<Vec<RpcAccessListItemView>>,
    value: Option<Vec<u8>>,
    data: Option<Vec<u8>>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct RpcAccessListItemView {
    address: Vec<u8>,
    storage_keys: Vec<Vec<u8>>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct RpcErrorView {
    code: u32,
    message: String,
    error_prefix: Option<String>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct RpcCallResultView {
    status: u8,
    gas_used: u64,
    return_data: Vec<u8>,
    revert_data: Option<Vec<u8>>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum RpcBlockTagView {
    Latest,
    Safe,
    Finalized,
    Earliest,
    Pending,
    Number(u64),
}

fn gateway_wasm_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/wasm32-unknown-unknown/release/ic_evm_gateway.wasm")
}

fn wrap_wasm_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/wasm32-unknown-unknown/release/wrap_canister.wasm")
}

fn mock_wrap_wasm_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/wasm32-unknown-unknown/release/mock_wrap_canister.wasm")
}

fn mock_ledger_wasm_path() -> PathBuf {
    let path = std::env::var_os("ICP_LEDGER_WASM")
        .unwrap_or_else(|| {
            panic!(
                "ICP_LEDGER_WASM must point to the official ic-icrc1-ledger.wasm; run scripts/prepare_ci_icrc1_ledger_wasm.sh first"
            )
        });
    let path = PathBuf::from(path);
    println!("using ICP_LEDGER_WASM at {:?}", path);
    path
}

fn read_wasm(path: PathBuf) -> Vec<u8> {
    if !path.exists() {
        panic!("wasm not found: build release wasm first: {path:?}");
    }
    std::fs::read(path).expect("read wasm")
}

fn test_caller() -> Principal {
    Principal::self_authenticating(b"wrap-unwrap-e2e-caller")
}

fn install_pair(pic: &PocketIc) -> (Principal, Principal, Principal) {
    let gateway_id = pic.create_canister();
    install_pair_with_options(pic, gateway_id, gateway_id, TEST_GENESIS_BALANCE_WEI)
}

fn set_allowed_assets(pic: &PocketIc, wrap_id: Principal, assets: Vec<Principal>) {
    let out = pic
        .update_call(
            wrap_id,
            test_caller(),
            "set_allowed_assets",
            Encode!(&assets).expect("encode allowed assets"),
        )
        .unwrap_or_else(|err| panic!("set_allowed_assets call failed: {err}"));
    let result: Result<(), String> = Decode!(&out, Result<(), String>)
        .expect("decode set_allowed_assets response");
    result.unwrap_or_else(|err| panic!("set_allowed_assets rejected: {err}"));
}

fn install_pair_with_options(
    pic: &PocketIc,
    gateway_id: Principal,
    wrap_gateway_id: Principal,
    wrap_evm_balance: u128,
) -> (Principal, Principal, Principal) {
    let wrap_id = pic.create_canister();
    let fee_ledger_id = pic.create_canister();
    let kasane_id = pic.create_canister();
    for canister_id in [gateway_id, wrap_id, fee_ledger_id, kasane_id] {
        pic.add_cycles(canister_id, 5_000_000_000_000u128);
    }

    let caller = test_caller();
    let caller_evm = hash::derive_evm_address_from_principal(caller.as_slice())
        .expect("derive caller evm address");
    let wrap_evm = hash::derive_evm_address_from_principal(wrap_id.as_slice())
        .expect("derive wrap evm address");
    let gateway_init = Some(GatewayInitArgs {
        genesis_balances: vec![
            GenesisBalanceView {
                address: caller_evm.to_vec(),
                amount: TEST_GENESIS_BALANCE_WEI,
            },
            GenesisBalanceView {
                address: wrap_evm.to_vec(),
                amount: wrap_evm_balance,
            },
        ],
        wrap_canister_id: wrap_id,
        wrap_factory_address: predict_create_address(caller_evm, 0).to_vec(),
        query_instruction_soft_limit: None,
        update_instruction_soft_limit: None,
    });
    let wrap_init = WrapInitArgs {
        kasane_canister: gateway_id,
        evm_gateway_canister: wrap_gateway_id,
        fee_ledger_canister: fee_ledger_id,
        wrap_factory_address: predict_create_address(caller_evm, 0).to_vec(),
        cycle_fee_e8s: 1_000_000,
        gas_price_buffer_bps: 12_000,
        allowed_assets: vec![fee_ledger_id],
    };
    let ledger_init = build_ledger_init(gateway_id, wrap_id, caller, TEST_LEDGER_BALANCE);

    pic.install_canister(
        fee_ledger_id,
        read_wasm(mock_ledger_wasm_path()),
        Encode!(&ledger_init).expect("encode ledger init"),
        None,
    );
    pic.install_canister(
        wrap_id,
        read_wasm(wrap_wasm_path()),
        Encode!(&wrap_init).expect("encode wrap init"),
        None,
    );
    pic.install_canister(
        gateway_id,
        read_wasm(gateway_wasm_path()),
        Encode!(&gateway_init).expect("encode gateway init"),
        None,
    );
    pic.set_controllers(wrap_id, Some(Principal::anonymous()), vec![caller])
        .unwrap_or_else(|err| panic!("set wrap controllers failed: {err}"));
    pic.set_controllers(gateway_id, Some(Principal::anonymous()), vec![caller])
        .unwrap_or_else(|err| panic!("set gateway controllers failed: {err}"));
    settle(pic, 6);
    (gateway_id, wrap_id, fee_ledger_id)
}

fn build_ledger_init(
    gateway_id: Principal,
    wrap_id: Principal,
    caller: Principal,
    wrap_balance: u128,
) -> LedgerArg {
    LedgerArg::Init(LedgerInitArgs {
        minting_account: LedgerAccount {
            owner: gateway_id,
            subaccount: None,
        },
        fee_collector_account: None,
        transfer_fee: Nat::from(10u64),
        decimals: Some(TEST_ASSET_DECIMALS),
        max_memo_length: None,
        token_symbol: "LICP".to_string(),
        token_name: "Local ICP".to_string(),
        metadata: Vec::new(),
        initial_balances: vec![
            (
                LedgerAccount {
                    owner: caller,
                    subaccount: None,
                },
                Nat::from(TEST_LEDGER_BALANCE),
            ),
            (
                LedgerAccount {
                    owner: wrap_id,
                    subaccount: None,
                },
                Nat::from(wrap_balance),
            ),
        ],
        feature_flags: Some(FeatureFlags { icrc2: true }),
        archive_options: ArchiveOptions {
            num_blocks_to_archive: 1_000,
            max_transactions_per_response: None,
            trigger_threshold: 2_000,
            max_message_size_bytes: None,
            cycles_for_archive_creation: Some(10_000_000_000_000),
            node_max_memory_size_bytes: None,
            controller_id: caller,
            more_controller_ids: None,
        },
        index_principal: None,
    })
}

fn install_mock_ledger(
    pic: &PocketIc,
    ledger_id: Principal,
    gateway_id: Principal,
    wrap_id: Principal,
    wrap_balance: u128,
) {
    pic.add_cycles(ledger_id, 5_000_000_000_000u128);
    let caller = test_caller();
    let ledger_init = build_ledger_init(gateway_id, wrap_id, caller, wrap_balance);
    pic.install_canister(
        ledger_id,
        read_wasm(mock_ledger_wasm_path()),
        Encode!(&ledger_init).expect("encode ledger init"),
        None,
    );
}

fn reinstall_mock_ledger(
    pic: &PocketIc,
    ledger_id: Principal,
    gateway_id: Principal,
    wrap_id: Principal,
    wrap_balance: u128,
) {
    let caller = test_caller();
    let ledger_init = build_ledger_init(gateway_id, wrap_id, caller, wrap_balance);
    pic.reinstall_canister(
        ledger_id,
        read_wasm(mock_ledger_wasm_path()),
        Encode!(&ledger_init).expect("encode ledger init"),
        None,
    )
    .unwrap_or_else(|err| panic!("reinstall mock ledger failed: {err}"));
    pic.start_canister(ledger_id, None)
        .unwrap_or_else(|err| panic!("start mock ledger failed: {err}"));
}

fn settle(pic: &PocketIc, rounds: usize) {
    for _ in 0..rounds {
        pic.advance_time(Duration::from_secs(60));
        pic.tick();
    }
}

fn build_contract_create_args(nonce: u64, data: Vec<u8>, gas_limit: u64) -> SubmitIcTxArgsDto {
    SubmitIcTxArgsDto {
        to: None,
        from: None,
        value: Nat::from(0u8),
        max_priority_fee_per_gas: Nat::from(300_000_000_000u64),
        data,
        max_fee_per_gas: Nat::from(600_000_000_000u64),
        nonce,
        gas_limit,
    }
}

fn submit_ic_tx(pic: &PocketIc, gateway_id: Principal, args: SubmitIcTxArgsDto) -> Vec<u8> {
    for _ in 0..4 {
        let out = pic
            .update_call(
                gateway_id,
                test_caller(),
                "submit_ic_tx",
                Encode!(&args).expect("encode submit"),
            )
            .unwrap();
        let result: Result<Vec<u8>, SubmitTxError> =
            Decode!(&out, Result<Vec<u8>, SubmitTxError>).expect("decode submit");
        match result {
            Ok(tx_id) => return tx_id,
            Err(SubmitTxError::Rejected(message)) if message == "ops.write.needs_migration" => {
                settle(pic, 1)
            }
            Err(err) => panic!("submit failed: {err:?}"),
        }
    }
    panic!("submit did not succeed after migration retries");
}

fn encode_unwrap_payload(asset: Principal, recipient: Principal) -> Vec<u8> {
    fn principal_field(principal: Principal) -> Vec<u8> {
        let bytes = principal.as_slice();
        let mut out = vec![0u8; 30];
        out[0] = bytes.len() as u8;
        out[1..1 + bytes.len()].copy_from_slice(bytes);
        out
    }

    let mut amount = [0u8; 32];
    amount[16..].copy_from_slice(&1_000_000_000_000u128.to_be_bytes());
    let mut out = Vec::with_capacity(93);
    out.push(1);
    out.extend_from_slice(&principal_field(asset));
    out.extend_from_slice(&amount);
    out.extend_from_slice(&principal_field(recipient));
    out
}

fn gateway_unwrap_request_ids_by_tx_id(
    pic: &PocketIc,
    gateway_id: Principal,
    tx_id: &[u8],
) -> Vec<Vec<u8>> {
    let out = pic
        .query_call(
            gateway_id,
            Principal::anonymous(),
            "get_unwrap_request_ids_by_tx_id",
            Encode!(&tx_id.to_vec()).expect("encode unwrap ids query"),
        )
        .unwrap();
    Decode!(&out, Vec<Vec<u8>>).expect("decode unwrap ids")
}

fn gateway_expected_nonce(pic: &PocketIc, gateway_id: Principal, address: [u8; 20]) -> u64 {
    let out = pic
        .query_call(
            gateway_id,
            Principal::anonymous(),
            "expected_nonce_by_address",
            Encode!(&address.to_vec()).expect("encode nonce query"),
        )
        .unwrap();
    let result: Result<u64, String> = Decode!(&out, Result<u64, String>).expect("decode nonce");
    result.expect("nonce query")
}

fn gateway_gas_price(pic: &PocketIc, gateway_id: Principal) -> u128 {
    let out = pic
        .query_call(
            gateway_id,
            Principal::anonymous(),
            "rpc_eth_gas_price",
            Encode!().unwrap(),
        )
        .unwrap();
    let result: Result<Nat, RpcErrorView> =
        Decode!(&out, Result<Nat, RpcErrorView>).expect("decode gas price");
    nat_to_u128(&result.expect("gas price"))
}

fn gateway_priority_fee(pic: &PocketIc, gateway_id: Principal) -> u128 {
    let out = pic
        .query_call(
            gateway_id,
            Principal::anonymous(),
            "rpc_eth_max_priority_fee_per_gas",
            Encode!().unwrap(),
        )
        .unwrap();
    let result: Result<Nat, RpcErrorView> =
        Decode!(&out, Result<Nat, RpcErrorView>).expect("decode priority fee");
    nat_to_u128(&result.expect("priority fee"))
}

fn gateway_estimate_gas(pic: &PocketIc, gateway_id: Principal, call: RpcCallObjectView) -> u64 {
    let out = pic
        .query_call(
            gateway_id,
            Principal::anonymous(),
            "rpc_eth_estimate_gas_object",
            Encode!(&call).expect("encode estimate gas"),
        )
        .unwrap();
    let result: Result<u64, RpcErrorView> =
        Decode!(&out, Result<u64, RpcErrorView>).expect("decode estimate gas");
    result.expect("estimate gas")
}

fn gateway_call(
    pic: &PocketIc,
    gateway_id: Principal,
    call: RpcCallObjectView,
) -> RpcCallResultView {
    let out = pic
        .query_call(
            gateway_id,
            Principal::anonymous(),
            "rpc_eth_call_object",
            Encode!(&call).expect("encode eth_call"),
        )
        .unwrap();
    let result = Decode!(&out, Result<RpcCallResultView, RpcErrorView>).expect("decode eth_call");
    result.expect("eth_call")
}

fn zero_value_word() -> Vec<u8> {
    vec![0u8; 32]
}

fn encode_balance_of(owner: [u8; 20]) -> Vec<u8> {
    let mut out = function_selector("balanceOf(address)").to_vec();
    out.extend_from_slice(&abi_word_from_address(owner));
    out
}

fn wrapped_token_balance_of(
    pic: &PocketIc,
    gateway_id: Principal,
    token: [u8; 20],
    owner: [u8; 20],
) -> u128 {
    let result = gateway_call(
        pic,
        gateway_id,
        RpcCallObjectView {
            to: Some(token.to_vec()),
            from: Some(owner.to_vec()),
            gas: None,
            gas_price: None,
            nonce: None,
            max_fee_per_gas: None,
            max_priority_fee_per_gas: None,
            chain_id: None,
            tx_type: None,
            access_list: None,
            value: Some(zero_value_word()),
            data: Some(encode_balance_of(owner)),
        },
    );
    assert_eq!(result.status, 1, "balanceOf eth_call must succeed");
    decode_u256_return_to_u128(&result.return_data)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TrackedBalances {
    caller_fee_ledger: u128,
    wrap_fee_ledger: u128,
    gateway_fee_ledger: u128,
    caller_wrapped_token: u128,
}

fn tracked_balances(
    pic: &PocketIc,
    gateway_id: Principal,
    fee_ledger_id: Principal,
    wrap_id: Principal,
    caller: Principal,
    caller_evm: [u8; 20],
    wrapped_token: Option<[u8; 20]>,
) -> TrackedBalances {
    TrackedBalances {
        caller_fee_ledger: ledger_balance_of(pic, fee_ledger_id, caller),
        wrap_fee_ledger: ledger_balance_of(pic, fee_ledger_id, wrap_id),
        gateway_fee_ledger: ledger_balance_of(pic, fee_ledger_id, gateway_id),
        caller_wrapped_token: wrapped_token
            .map(|token| {
                if gateway_get_code(pic, gateway_id, token).is_empty() {
                    0
                } else {
                    wrapped_token_balance_of(pic, gateway_id, token, caller_evm)
                }
            })
            .unwrap_or(0),
    }
}

fn gateway_get_code(pic: &PocketIc, gateway_id: Principal, address: [u8; 20]) -> Vec<u8> {
    let out = pic
        .query_call(
            gateway_id,
            Principal::anonymous(),
            "rpc_eth_get_code",
            Encode!(&address.to_vec(), &RpcBlockTagView::Latest).expect("encode get_code"),
        )
        .unwrap();
    let result: Result<Vec<u8>, RpcErrorView> =
        Decode!(&out, Result<Vec<u8>, RpcErrorView>).expect("decode get_code");
    result.expect("get_code")
}

fn gateway_receipt(
    pic: &PocketIc,
    gateway_id: Principal,
    tx_id: &[u8],
) -> Result<ReceiptView, LookupError> {
    let out = pic
        .query_call(
            gateway_id,
            Principal::anonymous(),
            "get_receipt",
            Encode!(&tx_id.to_vec()).expect("encode receipt query"),
        )
        .unwrap();
    Decode!(&out, Result<ReceiptView, LookupError>).expect("decode receipt")
}

fn gateway_pending_status(
    pic: &PocketIc,
    gateway_id: Principal,
    tx_id: &[u8],
) -> PendingStatusView {
    let out = pic
        .query_call(
            gateway_id,
            Principal::anonymous(),
            "get_pending",
            Encode!(&tx_id.to_vec()).expect("encode pending query"),
        )
        .unwrap();
    Decode!(&out, PendingStatusView).expect("decode pending status")
}

fn wait_for_receipt(pic: &PocketIc, gateway_id: Principal, tx_id: &[u8]) -> ReceiptView {
    let mut last_pending = None;
    for _ in 0..12 {
        settle(pic, 1);
        last_pending = Some(gateway_pending_status(pic, gateway_id, tx_id));
        match gateway_receipt(pic, gateway_id, tx_id) {
            Ok(receipt) => return receipt,
            Err(LookupError::Pending | LookupError::NotFound) => {}
            Err(err) => panic!("unexpected receipt lookup error: {err:?}"),
        }
    }
    panic!("receipt did not materialize; pending={last_pending:?}");
}

fn ledger_balance_of(pic: &PocketIc, fee_ledger_id: Principal, owner: Principal) -> u128 {
    let out = pic
        .query_call(
            fee_ledger_id,
            Principal::anonymous(),
            "icrc1_balance_of",
            Encode!(&LedgerAccount {
                owner,
                subaccount: None,
            })
            .expect("encode balance_of"),
        )
        .unwrap();
    let balance = Decode!(&out, Nat).expect("decode balance_of");
    nat_to_u128(&balance)
}

fn wrap_get_request(
    pic: &PocketIc,
    wrap_id: Principal,
    request_id: &[u8],
) -> Option<RequestOverview> {
    let out = pic
        .query_call(
            wrap_id,
            Principal::anonymous(),
            "get_request",
            Encode!(&request_id.to_vec()).expect("encode get_request"),
        )
        .unwrap();
    Decode!(&out, Option<RequestOverview>).expect("decode get_request")
}

fn wait_for_wrap_status(
    pic: &PocketIc,
    wrap_id: Principal,
    request_id: &[u8],
    expected: WrapRequestStatus,
) -> RequestOverview {
    let mut last = None;
    for _ in 0..40 {
        settle(pic, 1);
        let result = wrap_get_request(pic, wrap_id, request_id);
        last = result.clone();
        if let Some(overview) = result {
            if overview.status == expected {
                return overview;
            }
        }
    }
    panic!("wrap request did not reach expected status; last={last:?}");
}

fn wait_for_unwrap_status(
    pic: &PocketIc,
    wrap_id: Principal,
    request_id: &[u8],
    expected: WrapRequestStatus,
) -> RequestOverview {
    for _ in 0..20 {
        settle(pic, 1);
        if let Some(overview) = wrap_get_request(pic, wrap_id, request_id) {
            if overview.kind == RequestKind::Unwrap && overview.status == expected {
                return overview;
            }
        }
    }
    panic!("unwrap request did not reach expected status");
}

fn wrap_get_unwrap_requirements(
    pic: &PocketIc,
    wrap_id: Principal,
    args: &GetUnwrapRequirementsArgs,
) -> Result<GetUnwrapRequirementsOk, ApiError> {
    let out = pic
        .query_call(
            wrap_id,
            Principal::anonymous(),
            "get_unwrap_requirements",
            Encode!(args).expect("encode unwrap requirements"),
        )
        .unwrap();
    Decode!(&out, Result<GetUnwrapRequirementsOk, ApiError>).expect("decode unwrap requirements")
}

fn approve_fee_ledger_for_wrap(
    pic: &PocketIc,
    fee_ledger_id: Principal,
    wrap_id: Principal,
    amount: u128,
) {
    let out = pic
        .update_call(
            fee_ledger_id,
            test_caller(),
            "icrc2_approve",
            Encode!(&LedgerApproveArgs {
                fee: None,
                memo: None,
                from_subaccount: None,
                created_at_time: None,
                amount: Nat::from(amount),
                expected_allowance: None,
                expires_at: None,
                spender: LedgerAccount {
                    owner: wrap_id,
                    subaccount: None,
                },
            })
            .expect("encode ledger approve"),
        )
        .unwrap();
    let result: Result<Nat, LedgerApproveError> =
        Decode!(&out, Result<Nat, LedgerApproveError>).expect("decode ledger approve");
    assert!(result.is_ok(), "ledger approve failed: {result:?}");
}

fn wrap_factory_artifact_bytecode() -> Vec<u8> {
    let artifact = include_str!(
        "../../../tools/wrapper/contracts/out/WrapTokenFactory.sol/WrapTokenFactory.json"
    );
    let value: Value = serde_json::from_str(artifact).expect("parse factory artifact json");
    let object = value["bytecode"]["object"]
        .as_str()
        .expect("factory bytecode object");
    hex::decode(object.trim_start_matches("0x")).expect("decode factory bytecode")
}

fn predict_create_address(sender: [u8; 20], nonce: u64) -> [u8; 20] {
    assert_eq!(nonce, 0, "test helper only supports nonce 0");
    let mut payload = Vec::with_capacity(23);
    payload.push(0xd6);
    payload.push(0x94);
    payload.extend_from_slice(&sender);
    payload.push(0x80);
    let mut hasher = Keccak::v256();
    hasher.update(&payload);
    let mut out = [0u8; 32];
    hasher.finalize(&mut out);
    let mut address = [0u8; 20];
    address.copy_from_slice(&out[12..]);
    address
}

fn abi_word_from_u8(value: u8) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[31] = value;
    out
}

fn abi_word_from_u128(value: u128) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[16..].copy_from_slice(&value.to_be_bytes());
    out
}

fn abi_word_from_u64(value: u64) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[24..].copy_from_slice(&value.to_be_bytes());
    out
}

fn abi_word_from_address(address: [u8; 20]) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[12..].copy_from_slice(&address);
    out
}

fn function_selector(signature: &str) -> [u8; 4] {
    let mut hasher = Keccak::v256();
    hasher.update(signature.as_bytes());
    let mut out = [0u8; 32];
    hasher.finalize(&mut out);
    let mut selector = [0u8; 4];
    selector.copy_from_slice(&out[..4]);
    selector
}

fn encode_constructor_address(address: [u8; 20]) -> Vec<u8> {
    abi_word_from_address(address).to_vec()
}

fn encode_approve(factory: [u8; 20], amount: u128) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&function_selector("approve(address,uint256)"));
    out.extend_from_slice(&abi_word_from_address(factory));
    out.extend_from_slice(&abi_word_from_u128(amount));
    out
}

fn wrapped_token_artifact_bytecode() -> Vec<u8> {
    let artifact = include_str!(
        "../../../tools/wrapper/contracts/out/WrappedAssetToken.sol/WrappedAssetToken.json"
    );
    let value: Value = serde_json::from_str(artifact).expect("parse token artifact json");
    let object = value["bytecode"]["object"]
        .as_str()
        .expect("token bytecode object");
    hex::decode(object.trim_start_matches("0x")).expect("decode token bytecode")
}

fn abi_encode_bytes(bytes: &[u8]) -> Vec<u8> {
    let padded_len = bytes.len().div_ceil(32) * 32;
    let mut out = Vec::with_capacity(32 + padded_len);
    out.extend_from_slice(&abi_word_from_u64(bytes.len() as u64));
    let mut padded = vec![0u8; padded_len];
    padded[..bytes.len()].copy_from_slice(bytes);
    out.extend_from_slice(&padded);
    out
}

fn short_hex(data: &[u8]) -> String {
    let mut hasher = Keccak::v256();
    hasher.update(data);
    let mut out = [0u8; 32];
    hasher.finalize(&mut out);
    let mut text = String::with_capacity(16);
    for byte in out.iter().take(8) {
        text.push(char::from_digit((byte >> 4) as u32, 16).expect("hex hi"));
        text.push(char::from_digit((byte & 0x0f) as u32, 16).expect("hex lo"));
    }
    text
}

fn abi_encode_token_constructor(name: &str, symbol: &str, decimals: u8) -> Vec<u8> {
    let name_tail = abi_encode_bytes(name.as_bytes());
    let symbol_tail = abi_encode_bytes(symbol.as_bytes());
    let mut out = Vec::new();
    out.extend_from_slice(&abi_word_from_u64(96));
    out.extend_from_slice(&abi_word_from_u64(96 + name_tail.len() as u64));
    out.extend_from_slice(&abi_word_from_u8(decimals));
    out.extend_from_slice(&name_tail);
    out.extend_from_slice(&symbol_tail);
    out
}

fn predict_wrapped_token_address(factory: [u8; 20], asset: Principal, decimals: u8) -> [u8; 20] {
    let mut salt_hasher = Keccak::v256();
    salt_hasher.update(b"kasane.wrap.v1");
    salt_hasher.update(&abi_word_from_u64(TEST_CHAIN_ID));
    salt_hasher.update(asset.as_slice());
    let mut salt = [0u8; 32];
    salt_hasher.finalize(&mut salt);

    let suffix = short_hex(asset.as_slice());
    let mut init_code = wrapped_token_artifact_bytecode();
    init_code.extend_from_slice(&abi_encode_token_constructor(
        &format!("Kasane Wrapped {suffix}"),
        &format!("KW{suffix}"),
        decimals,
    ));

    let mut init_code_hash = [0u8; 32];
    let mut init_hasher = Keccak::v256();
    init_hasher.update(&init_code);
    init_hasher.finalize(&mut init_code_hash);

    let mut out = [0u8; 32];
    let mut hasher = Keccak::v256();
    hasher.update(&[0xff]);
    hasher.update(&factory);
    hasher.update(&salt);
    hasher.update(&init_code_hash);
    hasher.finalize(&mut out);
    let mut address = [0u8; 20];
    address.copy_from_slice(&out[12..]);
    address
}

fn deploy_factory(pic: &PocketIc, gateway_id: Principal, wrap_id: Principal) -> [u8; 20] {
    let caller = test_caller();
    let caller_evm =
        hash::derive_evm_address_from_principal(caller.as_slice()).expect("derive caller evm");
    let wrap_evm =
        hash::derive_evm_address_from_principal(wrap_id.as_slice()).expect("derive wrap evm");
    let factory = predict_create_address(caller_evm, 0);

    let mut deploy_data = wrap_factory_artifact_bytecode();
    deploy_data.extend_from_slice(&encode_constructor_address(wrap_evm));
    let deploy_tx = submit_ic_tx(
        pic,
        gateway_id,
        build_contract_create_args(0, deploy_data, 8_000_000),
    );
    let deploy_receipt = wait_for_receipt(pic, gateway_id, &deploy_tx);
    assert_eq!(deploy_receipt.status, 1, "factory deploy failed");
    assert_eq!(deploy_receipt.contract_address, Some(factory.to_vec()));
    factory
}

fn nat_to_u128(value: &Nat) -> u128 {
    let bytes = value.0.to_bytes_be();
    let mut out = [0u8; 16];
    let start = 16usize.saturating_sub(bytes.len());
    out[start..].copy_from_slice(bytes.as_slice());
    u128::from_be_bytes(out)
}

fn decode_u256_return_to_u128(bytes: &[u8]) -> u128 {
    assert!(
        bytes.len() <= 32,
        "unexpected u256 return size: {}",
        bytes.len()
    );
    assert!(
        bytes
            .iter()
            .take(bytes.len().saturating_sub(16))
            .all(|value| *value == 0),
        "u256 return value exceeds u128 range"
    );
    let mut out = [0u8; 16];
    let tail = &bytes[bytes.len().saturating_sub(16)..];
    out[16 - tail.len()..].copy_from_slice(tail);
    u128::from_be_bytes(out)
}

#[test]
fn wrap_submit_request_succeeds_with_real_ledger_and_factory() {
    let pic = PocketIc::new();
    let (gateway_id, wrap_id, fee_ledger_id) = install_pair(&pic);
    let caller = test_caller();
    let caller_evm =
        hash::derive_evm_address_from_principal(caller.as_slice()).expect("derive caller evm");
    let factory = deploy_factory(&pic, gateway_id, wrap_id);
    let token = predict_wrapped_token_address(factory, fee_ledger_id, TEST_ASSET_DECIMALS);
    let balances_before = tracked_balances(
        &pic,
        gateway_id,
        fee_ledger_id,
        wrap_id,
        caller,
        caller_evm,
        Some(token),
    );
    approve_fee_ledger_for_wrap(&pic, fee_ledger_id, wrap_id, WRAP_AMOUNT_E8S * 2);

    let args = SubmitWrapRequestArgs {
        asset_id: fee_ledger_id,
        amount_e8s: Nat::from(WRAP_AMOUNT_E8S),
        evm_recipient: caller_evm.to_vec(),
        evm_nonce: 0,
        gas_limit: TEST_WRAP_GAS_LIMIT,
    };
    let out = pic
        .update_call(
            wrap_id,
            caller,
            "submit_wrap_request",
            Encode!(&args).expect("encode wrap submit"),
        )
        .unwrap();
    let result: Result<SubmitWrapRequestOk, ApiError> =
        Decode!(&out, Result<SubmitWrapRequestOk, ApiError>).expect("decode wrap submit");
    let ok = result.expect("wrap submit should succeed");
    let overview =
        wait_for_wrap_status(&pic, wrap_id, &ok.request_id, WrapRequestStatus::Succeeded);
    let mint_tx_id = overview.mint_tx_id.expect("mint tx id should exist");
    let mint_receipt = wait_for_receipt(&pic, gateway_id, &mint_tx_id);
    assert_eq!(
        mint_receipt.status, 1,
        "mint receipt should succeed: {mint_receipt:?}"
    );
    assert!(
        !gateway_get_code(&pic, gateway_id, token).is_empty(),
        "wrapped token should be deployed"
    );
    let balances_after = tracked_balances(
        &pic,
        gateway_id,
        fee_ledger_id,
        wrap_id,
        caller,
        caller_evm,
        Some(token),
    );
    assert!(
        balances_after.caller_fee_ledger < balances_before.caller_fee_ledger,
        "caller fee-ledger balance should decrease after wrap"
    );
    assert_eq!(
        balances_after.wrap_fee_ledger,
        balances_before.wrap_fee_ledger + WRAP_AMOUNT_E8S + nat_to_u128(&ok.charged_fee_e8s)
    );
    assert_eq!(
        balances_after.gateway_fee_ledger,
        balances_before.gateway_fee_ledger
    );
    assert_eq!(
        balances_after.caller_wrapped_token,
        balances_before.caller_wrapped_token + WRAP_AMOUNT_E8S
    );
}

#[test]
fn unwrap_dispatch_succeeds_with_real_wrap_canister() {
    let pic = PocketIc::new();
    let (gateway_id, wrap_id, fee_ledger_id) = install_pair(&pic);
    let recipient = Principal::self_authenticating(b"wrap-unwrap-e2e-recipient");
    let caller_evm = hash::derive_evm_address_from_principal(test_caller().as_slice())
        .expect("derive caller evm");
    let factory = deploy_factory(&pic, gateway_id, wrap_id);
    approve_fee_ledger_for_wrap(&pic, fee_ledger_id, wrap_id, WRAP_AMOUNT_E8S * 2);
    let wrap_out = pic
        .update_call(
            wrap_id,
            test_caller(),
            "submit_wrap_request",
            Encode!(&SubmitWrapRequestArgs {
                asset_id: fee_ledger_id,
                amount_e8s: Nat::from(WRAP_AMOUNT_E8S),
                evm_recipient: caller_evm.to_vec(),
                evm_nonce: 0,
                gas_limit: TEST_WRAP_GAS_LIMIT,
            })
            .expect("encode wrap submit"),
        )
        .unwrap();
    let wrap_result: Result<SubmitWrapRequestOk, ApiError> =
        Decode!(&wrap_out, Result<SubmitWrapRequestOk, ApiError>).expect("decode wrap submit");
    let wrap_ok = wrap_result.expect("wrap submit should succeed");
    let overview = wait_for_wrap_status(
        &pic,
        wrap_id,
        &wrap_ok.request_id,
        WrapRequestStatus::Succeeded,
    );
    let mint_tx_id = overview.mint_tx_id.expect("mint tx id should exist");
    let mint_receipt = wait_for_receipt(&pic, gateway_id, &mint_tx_id);
    assert_eq!(
        mint_receipt.status, 1,
        "mint receipt should succeed: {mint_receipt:?}"
    );
    let token = predict_wrapped_token_address(factory, fee_ledger_id, TEST_ASSET_DECIMALS);
    assert!(
        !gateway_get_code(&pic, gateway_id, token).is_empty(),
        "wrapped token should be deployed after wrap"
    );
    let gas_price = gateway_gas_price(&pic, gateway_id);
    let priority_fee = gateway_priority_fee(&pic, gateway_id);
    let approve_nonce = gateway_expected_nonce(&pic, gateway_id, caller_evm);
    let approve_data = encode_approve(factory, WRAP_AMOUNT_E8S);
    let approve_gas = gateway_estimate_gas(
        &pic,
        gateway_id,
        RpcCallObjectView {
            to: Some(token.to_vec()),
            from: Some(caller_evm.to_vec()),
            gas: None,
            gas_price: None,
            nonce: Some(approve_nonce),
            max_fee_per_gas: None,
            max_priority_fee_per_gas: None,
            chain_id: None,
            tx_type: None,
            access_list: None,
            value: Some(zero_value_word()),
            data: Some(approve_data.clone()),
        },
    );
    let approve_tx = submit_ic_tx(
        &pic,
        gateway_id,
        SubmitIcTxArgsDto {
            to: Some(token.to_vec()),
            from: None,
            value: Nat::from(0u8),
            max_priority_fee_per_gas: Nat::from(priority_fee),
            data: approve_data,
            max_fee_per_gas: Nat::from(gas_price),
            nonce: approve_nonce,
            gas_limit: approve_gas.saturating_mul(12) / 10,
        },
    );
    let approve_receipt = wait_for_receipt(&pic, gateway_id, &approve_tx);
    assert_eq!(approve_receipt.status, 1, "approve tx failed");
    let unwrap_nonce = gateway_expected_nonce(&pic, gateway_id, caller_evm);
    let data = encode_unwrap_payload(fee_ledger_id, recipient);
    let unwrap_gas = gateway_estimate_gas(
        &pic,
        gateway_id,
        RpcCallObjectView {
            to: Some(WRAP_PRECOMPILE_ADDRESS.into_array().to_vec()),
            from: Some(caller_evm.to_vec()),
            gas: None,
            gas_price: None,
            nonce: Some(unwrap_nonce),
            max_fee_per_gas: None,
            max_priority_fee_per_gas: None,
            chain_id: None,
            tx_type: None,
            access_list: None,
            value: Some(zero_value_word()),
            data: Some(data.clone()),
        },
    );

    let tx_id = submit_ic_tx(
        &pic,
        gateway_id,
        SubmitIcTxArgsDto {
            to: Some(WRAP_PRECOMPILE_ADDRESS.into_array().to_vec()),
            from: None,
            value: Nat::from(0u8),
            max_priority_fee_per_gas: Nat::from(priority_fee),
            data,
            max_fee_per_gas: Nat::from(gas_price),
            nonce: unwrap_nonce,
            gas_limit: unwrap_gas.saturating_mul(12) / 10,
        },
    );

    let mut final_result = None;
    let mut last_result = None;
    let mut request_id = None;
    for _ in 0..12 {
        settle(&pic, 1);
        if request_id.is_none() {
            let request_ids = gateway_unwrap_request_ids_by_tx_id(&pic, gateway_id, &tx_id);
            if request_ids.len() == 1 {
                request_id = request_ids.into_iter().next();
            }
        }
        let Some(ref request_id) = request_id else {
            continue;
        };
        let out = pic
            .query_call(
                gateway_id,
                Principal::anonymous(),
                "get_unwrap_dispatch_overview",
                Encode!(request_id).unwrap(),
            )
            .unwrap();
        let result: Option<UnwrapDispatchOverviewView> =
            Decode!(&out, Option<UnwrapDispatchOverviewView>).expect("decode dispatch result");
        last_result = result.clone();
        if result.as_ref().map(|value| &value.status)
            == Some(&RequestDispatchStatusView::Dispatched)
        {
            final_result = result;
            break;
        }
    }

    let result = final_result
        .unwrap_or_else(|| panic!("unwrap should dispatch, last_result={last_result:?}"));
    assert_eq!(result.status, RequestDispatchStatusView::Dispatched);
    assert_eq!(result.error, None);
    let request_id = request_id.expect("unwrap request id must resolve from tx");
    let wrap_status = wrap_get_request(&pic, wrap_id, &request_id);
    assert_eq!(
        wrap_status.map(|value| value.status),
        Some(WrapRequestStatus::Queued)
    );
}

#[test]
fn unwrap_completes_and_credits_recipient_ledger_balance() {
    let pic = PocketIc::new();
    let (gateway_id, wrap_id, fee_ledger_id) = install_pair(&pic);
    let recipient = Principal::self_authenticating(b"wrap-unwrap-ledger-recipient");
    let caller = test_caller();
    let caller_evm =
        hash::derive_evm_address_from_principal(caller.as_slice()).expect("derive caller evm");
    let factory = deploy_factory(&pic, gateway_id, wrap_id);
    approve_fee_ledger_for_wrap(&pic, fee_ledger_id, wrap_id, WRAP_AMOUNT_E8S * 2);

    let wrap_out = pic
        .update_call(
            wrap_id,
            caller,
            "submit_wrap_request",
            Encode!(&SubmitWrapRequestArgs {
                asset_id: fee_ledger_id,
                amount_e8s: Nat::from(WRAP_AMOUNT_E8S),
                evm_recipient: caller_evm.to_vec(),
                evm_nonce: 0,
                gas_limit: TEST_WRAP_GAS_LIMIT,
            })
            .expect("encode wrap submit"),
        )
        .unwrap();
    let wrap_result: Result<SubmitWrapRequestOk, ApiError> =
        Decode!(&wrap_out, Result<SubmitWrapRequestOk, ApiError>).expect("decode wrap submit");
    let wrap_ok = wrap_result.expect("wrap submit should succeed");
    let overview = wait_for_wrap_status(
        &pic,
        wrap_id,
        &wrap_ok.request_id,
        WrapRequestStatus::Succeeded,
    );
    let mint_tx_id = overview.mint_tx_id.expect("mint tx id should exist");
    let mint_receipt = wait_for_receipt(&pic, gateway_id, &mint_tx_id);
    assert_eq!(
        mint_receipt.status, 1,
        "mint receipt should succeed: {mint_receipt:?}"
    );

    let token = predict_wrapped_token_address(factory, fee_ledger_id, TEST_ASSET_DECIMALS);
    let balances_after_wrap = tracked_balances(
        &pic,
        gateway_id,
        fee_ledger_id,
        wrap_id,
        caller,
        caller_evm,
        Some(token),
    );
    let gas_price = gateway_gas_price(&pic, gateway_id);
    let priority_fee = gateway_priority_fee(&pic, gateway_id);
    let approve_nonce = gateway_expected_nonce(&pic, gateway_id, caller_evm);
    let approve_data = encode_approve(factory, WRAP_AMOUNT_E8S);
    let approve_gas = gateway_estimate_gas(
        &pic,
        gateway_id,
        RpcCallObjectView {
            to: Some(token.to_vec()),
            from: Some(caller_evm.to_vec()),
            gas: None,
            gas_price: None,
            nonce: Some(approve_nonce),
            max_fee_per_gas: None,
            max_priority_fee_per_gas: None,
            chain_id: None,
            tx_type: None,
            access_list: None,
            value: Some(zero_value_word()),
            data: Some(approve_data.clone()),
        },
    );
    let approve_tx = submit_ic_tx(
        &pic,
        gateway_id,
        SubmitIcTxArgsDto {
            to: Some(token.to_vec()),
            from: None,
            value: Nat::from(0u8),
            max_priority_fee_per_gas: Nat::from(priority_fee),
            data: approve_data,
            max_fee_per_gas: Nat::from(gas_price),
            nonce: approve_nonce,
            gas_limit: approve_gas.saturating_mul(12) / 10,
        },
    );
    let approve_receipt = wait_for_receipt(&pic, gateway_id, &approve_tx);
    assert_eq!(approve_receipt.status, 1, "approve tx failed");

    let recipient_before = ledger_balance_of(&pic, fee_ledger_id, recipient);
    let unwrap_nonce = gateway_expected_nonce(&pic, gateway_id, caller_evm);
    let unwrap_data = encode_unwrap_payload(fee_ledger_id, recipient);
    let unwrap_gas = gateway_estimate_gas(
        &pic,
        gateway_id,
        RpcCallObjectView {
            to: Some(WRAP_PRECOMPILE_ADDRESS.into_array().to_vec()),
            from: Some(caller_evm.to_vec()),
            gas: None,
            gas_price: None,
            nonce: Some(unwrap_nonce),
            max_fee_per_gas: None,
            max_priority_fee_per_gas: None,
            chain_id: None,
            tx_type: None,
            access_list: None,
            value: Some(zero_value_word()),
            data: Some(unwrap_data.clone()),
        },
    );
    let tx_id = submit_ic_tx(
        &pic,
        gateway_id,
        SubmitIcTxArgsDto {
            to: Some(WRAP_PRECOMPILE_ADDRESS.into_array().to_vec()),
            from: None,
            value: Nat::from(0u8),
            max_priority_fee_per_gas: Nat::from(priority_fee),
            data: unwrap_data,
            max_fee_per_gas: Nat::from(gas_price),
            nonce: unwrap_nonce,
            gas_limit: unwrap_gas.saturating_mul(12) / 10,
        },
    );

    let mut request_id = None;
    for _ in 0..12 {
        settle(&pic, 1);
        let request_ids = gateway_unwrap_request_ids_by_tx_id(&pic, gateway_id, &tx_id);
        if request_ids.len() == 1 {
            request_id = request_ids.into_iter().next();
            break;
        }
    }
    let request_id = request_id.expect("unwrap request id must resolve from tx");
    let unwrap_overview =
        wait_for_unwrap_status(&pic, wrap_id, &request_id, WrapRequestStatus::Succeeded);
    assert_eq!(unwrap_overview.kind, RequestKind::Unwrap);
    assert!(
        unwrap_overview.ledger_tx_id.is_some(),
        "unwrap ledger tx should exist"
    );
    assert_eq!(unwrap_overview.error, None);

    let recipient_after = ledger_balance_of(&pic, fee_ledger_id, recipient);
    assert_eq!(recipient_before, 0);
    assert_eq!(recipient_after, WRAP_AMOUNT_E8S);
    let balances_after_unwrap = tracked_balances(
        &pic,
        gateway_id,
        fee_ledger_id,
        wrap_id,
        caller,
        caller_evm,
        Some(token),
    );
    assert_tracked_balance_delta(
        balances_after_wrap,
        balances_after_unwrap,
        0,
        -(WRAP_AMOUNT_E8S as i128) - 10,
        0,
        -(WRAP_AMOUNT_E8S as i128),
    );
}

fn assert_tracked_balance_delta(
    before: TrackedBalances,
    after: TrackedBalances,
    caller_fee_ledger_delta: i128,
    wrap_fee_ledger_delta: i128,
    gateway_fee_ledger_delta: i128,
    caller_wrapped_token_delta: i128,
) {
    assert_eq!(
        after.caller_fee_ledger as i128 - before.caller_fee_ledger as i128,
        caller_fee_ledger_delta,
        "unexpected caller fee-ledger delta"
    );
    assert_eq!(
        after.wrap_fee_ledger as i128 - before.wrap_fee_ledger as i128,
        wrap_fee_ledger_delta,
        "unexpected wrap fee-ledger delta"
    );
    assert_eq!(
        after.gateway_fee_ledger as i128 - before.gateway_fee_ledger as i128,
        gateway_fee_ledger_delta,
        "unexpected gateway fee-ledger delta"
    );
    assert_eq!(
        after.caller_wrapped_token as i128 - before.caller_wrapped_token as i128,
        caller_wrapped_token_delta,
        "unexpected wrapped token delta"
    );
}

#[test]
fn unwrap_requirements_report_readiness_transitions() {
    let pic = PocketIc::new();
    let (gateway_id, wrap_id, fee_ledger_id) = install_pair(&pic);
    let caller_evm = hash::derive_evm_address_from_principal(test_caller().as_slice())
        .expect("derive caller evm");
    let factory = deploy_factory(&pic, gateway_id, wrap_id);

    let missing = wrap_get_unwrap_requirements(
        &pic,
        wrap_id,
        &GetUnwrapRequirementsArgs {
            asset_id: fee_ledger_id,
            amount_e8s: Nat::from(WRAP_AMOUNT_E8S),
            caller_evm_address: caller_evm.to_vec(),
        },
    )
    .expect("requirements before wrap");
    assert_eq!(missing.factory_address, factory.to_vec());
    assert_eq!(missing.wrapped_token_address, None);
    assert_eq!(missing.readiness, UnwrapReadiness::TokenNotDeployed);
    assert!(!missing.approve_required);

    approve_fee_ledger_for_wrap(&pic, fee_ledger_id, wrap_id, WRAP_AMOUNT_E8S * 2);
    let wrap_out = pic
        .update_call(
            wrap_id,
            test_caller(),
            "submit_wrap_request",
            Encode!(&SubmitWrapRequestArgs {
                asset_id: fee_ledger_id,
                amount_e8s: Nat::from(WRAP_AMOUNT_E8S),
                evm_recipient: caller_evm.to_vec(),
                evm_nonce: 0,
                gas_limit: TEST_WRAP_GAS_LIMIT,
            })
            .expect("encode wrap submit"),
        )
        .unwrap();
    let wrap_result: Result<SubmitWrapRequestOk, ApiError> =
        Decode!(&wrap_out, Result<SubmitWrapRequestOk, ApiError>).expect("decode wrap submit");
    let wrap_ok = wrap_result.expect("wrap submit should succeed");
    let overview = wait_for_wrap_status(
        &pic,
        wrap_id,
        &wrap_ok.request_id,
        WrapRequestStatus::Succeeded,
    );
    let mint_tx_id = overview.mint_tx_id.expect("mint tx id should exist");
    let mint_receipt = wait_for_receipt(&pic, gateway_id, &mint_tx_id);
    assert_eq!(
        mint_receipt.status, 1,
        "mint receipt should succeed: {mint_receipt:?}"
    );

    let allowance_missing = wrap_get_unwrap_requirements(
        &pic,
        wrap_id,
        &GetUnwrapRequirementsArgs {
            asset_id: fee_ledger_id,
            amount_e8s: Nat::from(WRAP_AMOUNT_E8S),
            caller_evm_address: caller_evm.to_vec(),
        },
    )
    .expect("requirements after wrap");
    assert_eq!(
        allowance_missing.wrapped_token_address,
        Some(predict_wrapped_token_address(factory, fee_ledger_id, TEST_ASSET_DECIMALS).to_vec())
    );
    assert_eq!(nat_to_u128(&allowance_missing.balance), WRAP_AMOUNT_E8S);
    assert_eq!(nat_to_u128(&allowance_missing.allowance), 0);
    assert_eq!(
        allowance_missing.readiness,
        UnwrapReadiness::InsufficientAllowance
    );
    assert!(allowance_missing.approve_required);

    let approve_nonce = gateway_expected_nonce(&pic, gateway_id, caller_evm);
    let approve_data = encode_approve(factory, WRAP_AMOUNT_E8S);
    let approve_gas = gateway_estimate_gas(
        &pic,
        gateway_id,
        RpcCallObjectView {
            to: allowance_missing.wrapped_token_address.clone(),
            from: Some(caller_evm.to_vec()),
            gas: None,
            gas_price: None,
            nonce: Some(approve_nonce),
            max_fee_per_gas: None,
            max_priority_fee_per_gas: None,
            chain_id: None,
            tx_type: None,
            access_list: None,
            value: Some(zero_value_word()),
            data: Some(approve_data.clone()),
        },
    );
    let approve_tx = submit_ic_tx(
        &pic,
        gateway_id,
        SubmitIcTxArgsDto {
            to: allowance_missing.wrapped_token_address.clone(),
            from: None,
            value: Nat::from(0u8),
            max_priority_fee_per_gas: Nat::from(gateway_priority_fee(&pic, gateway_id)),
            data: approve_data,
            max_fee_per_gas: Nat::from(gateway_gas_price(&pic, gateway_id)),
            nonce: approve_nonce,
            gas_limit: approve_gas.saturating_mul(12) / 10,
        },
    );
    let approve_receipt = wait_for_receipt(&pic, gateway_id, &approve_tx);
    assert_eq!(approve_receipt.status, 1, "approve tx failed");

    let insufficient_balance = wrap_get_unwrap_requirements(
        &pic,
        wrap_id,
        &GetUnwrapRequirementsArgs {
            asset_id: fee_ledger_id,
            amount_e8s: Nat::from(WRAP_AMOUNT_E8S + 1),
            caller_evm_address: caller_evm.to_vec(),
        },
    )
    .expect("requirements for larger amount");
    assert_eq!(
        insufficient_balance.readiness,
        UnwrapReadiness::InsufficientBalance
    );
    assert!(!insufficient_balance.approve_required);

    let ready = wrap_get_unwrap_requirements(
        &pic,
        wrap_id,
        &GetUnwrapRequirementsArgs {
            asset_id: fee_ledger_id,
            amount_e8s: Nat::from(WRAP_AMOUNT_E8S),
            caller_evm_address: caller_evm.to_vec(),
        },
    )
    .expect("requirements after approve");
    assert_eq!(ready.readiness, UnwrapReadiness::Ready);
    assert!(!ready.approve_required);
    assert_eq!(nat_to_u128(&ready.balance), WRAP_AMOUNT_E8S);
    assert_eq!(nat_to_u128(&ready.allowance), WRAP_AMOUNT_E8S);
}

#[test]
fn unwrap_dispatch_marks_request_failed_when_asset_ledger_is_missing() {
    let pic = PocketIc::new();
    let (gateway_id, wrap_id, _) = install_pair(&pic);
    let missing_ledger = pic.create_canister();
    set_allowed_assets(&pic, wrap_id, vec![missing_ledger]);
    let request_id = vec![0xabu8; 32];
    let out = pic
        .update_call(
            wrap_id,
            gateway_id,
            "dispatch_unwrap_request",
            Encode!(&DispatchUnwrapRequestArgs {
                request_id: request_id.clone(),
                asset_id: missing_ledger,
                amount_e8s: Nat::from(WRAP_AMOUNT_E8S),
                recipient: test_caller(),
            })
            .expect("encode dispatch unwrap"),
        )
        .unwrap();
    let result: Result<DispatchUnwrapRequestOk, ApiError> = Decode!(
        &out,
        Result<DispatchUnwrapRequestOk, ApiError>
    )
    .expect("decode dispatch unwrap");
    let ok = result.expect("dispatch should accept queue insertion");
    assert_eq!(ok.request_id, request_id);

    let overview = wait_for_unwrap_status(&pic, wrap_id, &request_id, WrapRequestStatus::Failed);
    assert_eq!(overview.kind, RequestKind::Unwrap);
    assert_eq!(overview.ledger_tx_id, None);
    let error = overview
        .error
        .expect("unwrap failure should expose an error");
    assert!(
        error.code.starts_with("ledger.call_failed:"),
        "unexpected unwrap failure code: {}",
        error.code
    );
}

#[test]
fn unwrap_retry_request_succeeds_after_ledger_is_installed() {
    let pic = PocketIc::new();
    let (gateway_id, wrap_id, _) = install_pair(&pic);
    let delayed_ledger_id = pic.create_canister();
    set_allowed_assets(&pic, wrap_id, vec![delayed_ledger_id]);
    let request_id = vec![0x42u8; 32];

    let out = pic
        .update_call(
            wrap_id,
            gateway_id,
            "dispatch_unwrap_request",
            Encode!(&DispatchUnwrapRequestArgs {
                request_id: request_id.clone(),
                asset_id: delayed_ledger_id,
                amount_e8s: Nat::from(WRAP_AMOUNT_E8S),
                recipient: test_caller(),
            })
            .expect("encode dispatch unwrap"),
        )
        .unwrap();
    let result: Result<DispatchUnwrapRequestOk, ApiError> = Decode!(
        &out,
        Result<DispatchUnwrapRequestOk, ApiError>
    )
    .expect("decode dispatch unwrap");
    result.expect("dispatch should enqueue");

    let failed = wait_for_unwrap_status(&pic, wrap_id, &request_id, WrapRequestStatus::Failed);
    let failed_error = failed.error.expect("failed unwrap should expose error");
    assert!(
        failed_error.code.starts_with("ledger.call_failed:"),
        "unexpected unwrap failure code: {}",
        failed_error.code
    );

    install_mock_ledger(
        &pic,
        delayed_ledger_id,
        gateway_id,
        wrap_id,
        WRAP_AMOUNT_E8S * 2,
    );
    let recipient_before = ledger_balance_of(&pic, delayed_ledger_id, test_caller());
    let retry_out = pic
        .update_call(
            wrap_id,
            test_caller(),
            "retry_request",
            Encode!(&RetryRequestArgs {
                request_id: request_id.clone(),
            })
            .expect("encode retry request"),
        )
        .unwrap();
    let retry_result: Result<RequestOverview, ApiError> =
        Decode!(&retry_out, Result<RequestOverview, ApiError>).expect("decode retry request");
    let retry_ok = retry_result.expect("retry should succeed after ledger install");
    assert_eq!(retry_ok.status, WrapRequestStatus::Succeeded);
    assert!(
        retry_ok.ledger_tx_id.is_some(),
        "retry should produce ledger tx"
    );

    let recipient_after = ledger_balance_of(&pic, delayed_ledger_id, test_caller());
    assert_eq!(recipient_after - recipient_before, WRAP_AMOUNT_E8S);
}

#[test]
fn unwrap_burn_then_ledger_transfer_failure_is_retryable_without_double_refund() {
    let pic = PocketIc::new();
    let (gateway_id, wrap_id, fee_ledger_id) = install_pair(&pic);
    let broken_ledger_id = pic.create_canister();
    set_allowed_assets(&pic, wrap_id, vec![fee_ledger_id, broken_ledger_id]);
    let caller = test_caller();
    let caller_evm =
        hash::derive_evm_address_from_principal(caller.as_slice()).expect("derive caller evm");
    let factory = deploy_factory(&pic, gateway_id, wrap_id);
    install_mock_ledger(
        &pic,
        broken_ledger_id,
        gateway_id,
        wrap_id,
        WRAP_AMOUNT_E8S * 2,
    );
    approve_fee_ledger_for_wrap(&pic, fee_ledger_id, wrap_id, WRAP_AMOUNT_E8S * 2);
    approve_fee_ledger_for_wrap(&pic, broken_ledger_id, wrap_id, WRAP_AMOUNT_E8S * 2);

    let wrap_out = pic
        .update_call(
            wrap_id,
            caller,
            "submit_wrap_request",
            Encode!(&SubmitWrapRequestArgs {
                asset_id: broken_ledger_id,
                amount_e8s: Nat::from(WRAP_AMOUNT_E8S),
                evm_recipient: caller_evm.to_vec(),
                evm_nonce: 0,
                gas_limit: TEST_WRAP_GAS_LIMIT,
            })
            .expect("encode wrap submit"),
        )
        .unwrap();
    let wrap_result: Result<SubmitWrapRequestOk, ApiError> =
        Decode!(&wrap_out, Result<SubmitWrapRequestOk, ApiError>).expect("decode wrap submit");
    let wrap_ok = wrap_result.expect("wrap submit should succeed");
    let wrap_overview = wait_for_wrap_status(
        &pic,
        wrap_id,
        &wrap_ok.request_id,
        WrapRequestStatus::Succeeded,
    );
    let mint_tx_id = wrap_overview.mint_tx_id.expect("mint tx id should exist");
    let mint_receipt = wait_for_receipt(&pic, gateway_id, &mint_tx_id);
    assert_eq!(mint_receipt.status, 1, "mint receipt should succeed");

    let token = predict_wrapped_token_address(factory, broken_ledger_id, TEST_ASSET_DECIMALS);
    let after_wrap = tracked_balances(
        &pic,
        gateway_id,
        broken_ledger_id,
        wrap_id,
        caller,
        caller_evm,
        Some(token),
    );
    assert_eq!(
        after_wrap.wrap_fee_ledger,
        (WRAP_AMOUNT_E8S * 2) + WRAP_AMOUNT_E8S
    );
    assert_eq!(after_wrap.caller_wrapped_token, WRAP_AMOUNT_E8S);

    let approve_nonce = gateway_expected_nonce(&pic, gateway_id, caller_evm);
    let approve_data = encode_approve(factory, WRAP_AMOUNT_E8S);
    let approve_gas = gateway_estimate_gas(
        &pic,
        gateway_id,
        RpcCallObjectView {
            to: Some(token.to_vec()),
            from: Some(caller_evm.to_vec()),
            gas: None,
            gas_price: None,
            nonce: Some(approve_nonce),
            max_fee_per_gas: None,
            max_priority_fee_per_gas: None,
            chain_id: None,
            tx_type: None,
            access_list: None,
            value: Some(zero_value_word()),
            data: Some(approve_data.clone()),
        },
    );
    let approve_tx = submit_ic_tx(
        &pic,
        gateway_id,
        SubmitIcTxArgsDto {
            to: Some(token.to_vec()),
            from: None,
            value: Nat::from(0u8),
            max_priority_fee_per_gas: Nat::from(gateway_priority_fee(&pic, gateway_id)),
            data: approve_data,
            max_fee_per_gas: Nat::from(gateway_gas_price(&pic, gateway_id)),
            nonce: approve_nonce,
            gas_limit: approve_gas.saturating_mul(12) / 10,
        },
    );
    let approve_receipt = wait_for_receipt(&pic, gateway_id, &approve_tx);
    assert_eq!(approve_receipt.status, 1, "approve tx failed");

    pic.stop_canister(broken_ledger_id, None)
        .unwrap_or_else(|err| panic!("stop broken ledger failed: {err}"));

    let unwrap_nonce = gateway_expected_nonce(&pic, gateway_id, caller_evm);
    let unwrap_data = encode_unwrap_payload(broken_ledger_id, caller);
    let unwrap_gas = gateway_estimate_gas(
        &pic,
        gateway_id,
        RpcCallObjectView {
            to: Some(WRAP_PRECOMPILE_ADDRESS.into_array().to_vec()),
            from: Some(caller_evm.to_vec()),
            gas: None,
            gas_price: None,
            nonce: Some(unwrap_nonce),
            max_fee_per_gas: None,
            max_priority_fee_per_gas: None,
            chain_id: None,
            tx_type: None,
            access_list: None,
            value: Some(zero_value_word()),
            data: Some(unwrap_data.clone()),
        },
    );
    let tx_id = submit_ic_tx(
        &pic,
        gateway_id,
        SubmitIcTxArgsDto {
            to: Some(WRAP_PRECOMPILE_ADDRESS.into_array().to_vec()),
            from: None,
            value: Nat::from(0u8),
            max_priority_fee_per_gas: Nat::from(gateway_priority_fee(&pic, gateway_id)),
            data: unwrap_data,
            max_fee_per_gas: Nat::from(gateway_gas_price(&pic, gateway_id)),
            nonce: unwrap_nonce,
            gas_limit: unwrap_gas.saturating_mul(12) / 10,
        },
    );

    let mut request_id = None;
    for _ in 0..12 {
        settle(&pic, 1);
        let request_ids = gateway_unwrap_request_ids_by_tx_id(&pic, gateway_id, &tx_id);
        if request_ids.len() == 1 {
            request_id = request_ids.into_iter().next();
            break;
        }
    }
    let request_id = request_id.expect("unwrap request id must resolve from tx");
    let failed = wait_for_unwrap_status(&pic, wrap_id, &request_id, WrapRequestStatus::Failed);
    let failed_error = failed.error.expect("failed unwrap should expose an error");
    assert!(
        failed_error.code.starts_with("ledger.call_failed:"),
        "unexpected unwrap failure code: {}",
        failed_error.code
    );
    assert_eq!(
        wrapped_token_balance_of(&pic, gateway_id, token, caller_evm),
        0,
        "burned wrapped tokens must not reappear after ledger failure"
    );

    reinstall_mock_ledger(
        &pic,
        broken_ledger_id,
        gateway_id,
        wrap_id,
        after_wrap.wrap_fee_ledger,
    );
    let after_reinstall = tracked_balances(
        &pic,
        gateway_id,
        broken_ledger_id,
        wrap_id,
        caller,
        caller_evm,
        Some(token),
    );
    assert_eq!(after_reinstall.wrap_fee_ledger, after_wrap.wrap_fee_ledger);
    assert_eq!(after_reinstall.caller_wrapped_token, 0);
    let recipient_before = ledger_balance_of(&pic, broken_ledger_id, caller);

    let retry_out = pic
        .update_call(
            wrap_id,
            caller,
            "retry_request",
            Encode!(&RetryRequestArgs {
                request_id: request_id.clone(),
            })
            .expect("encode retry request"),
        )
        .unwrap();
    let retry_result: Result<RequestOverview, ApiError> =
        Decode!(&retry_out, Result<RequestOverview, ApiError>).expect("decode retry request");
    let retry_ok = retry_result.expect("retry should succeed after ledger reinstall");
    assert_eq!(retry_ok.status, WrapRequestStatus::Succeeded);
    assert!(
        retry_ok.ledger_tx_id.is_some(),
        "retry should produce ledger tx"
    );

    let recipient_after = ledger_balance_of(&pic, broken_ledger_id, caller);
    assert_eq!(recipient_after - recipient_before, WRAP_AMOUNT_E8S);
    let after_retry = tracked_balances(
        &pic,
        gateway_id,
        broken_ledger_id,
        wrap_id,
        caller,
        caller_evm,
        Some(token),
    );
    assert_eq!(after_retry.caller_wrapped_token, 0);
    assert_eq!(
        after_retry.wrap_fee_ledger,
        after_wrap.wrap_fee_ledger - WRAP_AMOUNT_E8S - 10,
        "retry should produce exactly one ledger refund and one transfer fee"
    );
}

#[test]
fn wrap_recover_failed_wrap_returns_funds_after_gateway_reinstall_breaks_mint_step() {
    let pic = PocketIc::new();
    let (gateway_id, wrap_id, fee_ledger_id) = install_pair(&pic);
    let caller = test_caller();
    let caller_evm =
        hash::derive_evm_address_from_principal(caller.as_slice()).expect("derive caller evm");
    let _factory = deploy_factory(&pic, gateway_id, wrap_id);
    approve_fee_ledger_for_wrap(&pic, fee_ledger_id, wrap_id, WRAP_AMOUNT_E8S * 2);
    let balances_before = tracked_balances(
        &pic,
        gateway_id,
        fee_ledger_id,
        wrap_id,
        caller,
        caller_evm,
        None,
    );

    let caller_before = ledger_balance_of(&pic, fee_ledger_id, caller);
    let wrap_before = ledger_balance_of(&pic, fee_ledger_id, wrap_id);
    let wrap_out = pic
        .update_call(
            wrap_id,
            caller,
            "submit_wrap_request",
            Encode!(&SubmitWrapRequestArgs {
                asset_id: fee_ledger_id,
                amount_e8s: Nat::from(WRAP_AMOUNT_E8S),
                evm_recipient: caller_evm.to_vec(),
                evm_nonce: 0,
                gas_limit: TEST_WRAP_GAS_LIMIT,
            })
            .expect("encode wrap submit"),
        )
        .unwrap();
    let wrap_result: Result<SubmitWrapRequestOk, ApiError> =
        Decode!(&wrap_out, Result<SubmitWrapRequestOk, ApiError>).expect("decode wrap submit");
    let wrap_ok = wrap_result.expect("wrap submit should accept and queue");

    pic.reinstall_canister(
        gateway_id,
        read_wasm(mock_wrap_wasm_path()),
        Encode!(&()).expect("encode mock wrap init"),
        Some(caller),
    )
    .unwrap_or_else(|err| panic!("reinstall gateway failed: {err}"));

    let failed = wait_for_wrap_status(
        &pic,
        wrap_id,
        &wrap_ok.request_id,
        WrapRequestStatus::Failed,
    );
    assert!(
        failed.pull_ledger_tx_id.is_some(),
        "failed wrap should have pulled ledger funds"
    );
    assert!(
        failed.mint_tx_id.is_none(),
        "mint tx must be absent on recoverable failure"
    );
    let failed_error = failed.error.expect("failed wrap should expose error");
    assert!(
        failed_error.code.contains("nonce_call_failed")
            || failed_error.code.contains("nonce_decode_failed"),
        "unexpected wrap failure code: {}",
        failed_error.code
    );

    let caller_mid = ledger_balance_of(&pic, fee_ledger_id, caller);
    let wrap_mid = ledger_balance_of(&pic, fee_ledger_id, wrap_id);
    assert!(
        caller_mid < caller_before,
        "caller balance must decrease after pull"
    );
    assert!(
        wrap_mid > wrap_before,
        "wrap balance must increase after pull"
    );

    let recover_out = pic
        .update_call(
            wrap_id,
            caller,
            "recover_failed_wrap",
            Encode!(&RecoverFailedWrapArgs {
                request_id: wrap_ok.request_id.clone(),
            })
            .expect("encode recover failed wrap"),
        )
        .unwrap();
    let recover_result: Result<RequestOverview, ApiError> =
        Decode!(&recover_out, Result<RequestOverview, ApiError>)
            .expect("decode recover failed wrap");
    let recovered = recover_result.expect("recover_failed_wrap should succeed");
    assert_eq!(recovered.status, WrapRequestStatus::Failed);
    assert!(
        recovered.withdraw_ledger_tx_id.is_some(),
        "withdraw tx should exist"
    );

    let caller_after = ledger_balance_of(&pic, fee_ledger_id, caller);
    let wrap_after = ledger_balance_of(&pic, fee_ledger_id, wrap_id);
    assert_eq!(
        caller_after - caller_mid,
        WRAP_AMOUNT_E8S,
        "recover must return the pulled principal amount"
    );
    assert!(
        caller_after > caller_mid,
        "caller balance must increase after recover"
    );
    assert!(
        wrap_after < wrap_mid,
        "wrap balance must decrease after recover"
    );
    assert!(
        caller_after < caller_before,
        "charged wrap fee should still remain deducted after recover"
    );
    let balances_after = tracked_balances(
        &pic,
        gateway_id,
        fee_ledger_id,
        wrap_id,
        caller,
        caller_evm,
        None,
    );
    assert!(
        balances_after.caller_fee_ledger < balances_before.caller_fee_ledger,
        "caller fee-ledger balance should only retain wrap-side charges after recover"
    );
    assert_eq!(
        balances_after.wrap_fee_ledger,
        balances_before.wrap_fee_ledger + nat_to_u128(&wrap_ok.charged_fee_e8s) - 10
    );
    assert_eq!(
        balances_after.gateway_fee_ledger,
        balances_before.gateway_fee_ledger
    );
    assert_eq!(
        balances_after.caller_wrapped_token,
        balances_before.caller_wrapped_token
    );

    let recover_again_out = pic
        .update_call(
            wrap_id,
            caller,
            "recover_failed_wrap",
            Encode!(&RecoverFailedWrapArgs {
                request_id: wrap_ok.request_id.clone(),
            })
            .expect("encode recover failed wrap retry"),
        )
        .unwrap();
    let recover_again_result: Result<RequestOverview, ApiError> =
        Decode!(&recover_again_out, Result<RequestOverview, ApiError>)
            .expect("decode recover failed wrap retry");
    let recover_again_err = recover_again_result.expect_err("second recover should fail");
    match recover_again_err {
        ApiError::InvalidArgument(detail) => {
            assert_eq!(detail.code, "withdraw.already_withdrawn");
        }
        other => panic!("unexpected second recover error: {other:?}"),
    }
}

#[test]
fn unwrap_retry_request_rejects_concurrent_second_call_while_first_is_in_progress() {
    let pic = PocketIc::new();
    let (gateway_id, wrap_id, _) = install_pair(&pic);
    let delayed_ledger_id = pic.create_canister();
    set_allowed_assets(&pic, wrap_id, vec![delayed_ledger_id]);
    let caller = test_caller();
    let request_id = vec![0x55u8; 32];

    let out = pic
        .update_call(
            wrap_id,
            gateway_id,
            "dispatch_unwrap_request",
            Encode!(&DispatchUnwrapRequestArgs {
                request_id: request_id.clone(),
                asset_id: delayed_ledger_id,
                amount_e8s: Nat::from(WRAP_AMOUNT_E8S),
                recipient: caller,
            })
            .expect("encode dispatch unwrap"),
        )
        .unwrap();
    let result: Result<DispatchUnwrapRequestOk, ApiError> =
        Decode!(&out, Result<DispatchUnwrapRequestOk, ApiError>).expect("decode dispatch unwrap");
    result.expect("dispatch should enqueue");

    let failed = wait_for_unwrap_status(&pic, wrap_id, &request_id, WrapRequestStatus::Failed);
    let failed_error = failed.error.expect("failed unwrap should expose error");
    assert!(
        failed_error.code.starts_with("ledger.call_failed:"),
        "unexpected unwrap failure code: {}",
        failed_error.code
    );

    install_mock_ledger(
        &pic,
        delayed_ledger_id,
        gateway_id,
        wrap_id,
        WRAP_AMOUNT_E8S * 2,
    );
    let recipient_before = ledger_balance_of(&pic, delayed_ledger_id, caller);
    let wrap_before = ledger_balance_of(&pic, delayed_ledger_id, wrap_id);

    let retry_arg = Encode!(&RetryRequestArgs {
        request_id: request_id.clone(),
    })
    .expect("encode retry arg");
    let first_call = pic
        .submit_call(wrap_id, caller, "retry_request", retry_arg.clone())
        .unwrap_or_else(|err| panic!("submit first retry failed: {err}"));
    let second_call = pic
        .submit_call(wrap_id, caller, "retry_request", retry_arg)
        .unwrap_or_else(|err| panic!("submit second retry failed: {err}"));

    let first_out = pic
        .await_call(first_call)
        .unwrap_or_else(|err| panic!("await first retry failed: {err:?}"));
    let second_out = pic
        .await_call(second_call)
        .unwrap_or_else(|err| panic!("await second retry failed: {err:?}"));

    let first_result: Result<RequestOverview, ApiError> =
        Decode!(&first_out, Result<RequestOverview, ApiError>).expect("decode first retry");
    let second_result: Result<RequestOverview, ApiError> =
        Decode!(&second_out, Result<RequestOverview, ApiError>).expect("decode second retry");
    let (successes, failures): (Vec<_>, Vec<_>) = [first_result, second_result]
        .into_iter()
        .partition(Result::is_ok);

    assert_eq!(successes.len(), 1, "exactly one retry should succeed");
    assert_eq!(failures.len(), 1, "exactly one retry should fail");
    let retried = successes
        .into_iter()
        .next()
        .expect("missing successful retry")
        .expect("successful retry payload");
    assert_eq!(retried.status, WrapRequestStatus::Succeeded);
    assert!(
        retried.ledger_tx_id.is_some(),
        "retry should record ledger tx"
    );
    let retry_err = failures
        .into_iter()
        .next()
        .expect("missing failed retry")
        .expect_err("failed retry payload");
    match retry_err {
        ApiError::InvalidArgument(detail) => {
            assert_eq!(detail.code, "unwrap.retry_already_running");
        }
        other => panic!("unexpected concurrent retry error: {other:?}"),
    }

    let recipient_after = ledger_balance_of(&pic, delayed_ledger_id, caller);
    let wrap_after = ledger_balance_of(&pic, delayed_ledger_id, wrap_id);
    assert_eq!(recipient_after - recipient_before, WRAP_AMOUNT_E8S);
    assert_eq!(
        wrap_before - wrap_after,
        WRAP_AMOUNT_E8S + 10,
        "wrap canister should pay one retry refund plus one transfer fee"
    );
}

#[test]
fn wrap_recover_failed_wrap_rejects_concurrent_second_call_while_first_is_in_progress() {
    let pic = PocketIc::new();
    let (gateway_id, wrap_id, fee_ledger_id) = install_pair(&pic);
    let caller = test_caller();
    let caller_evm =
        hash::derive_evm_address_from_principal(caller.as_slice()).expect("derive caller evm");
    let _factory = deploy_factory(&pic, gateway_id, wrap_id);
    approve_fee_ledger_for_wrap(&pic, fee_ledger_id, wrap_id, WRAP_AMOUNT_E8S * 2);

    let caller_before = ledger_balance_of(&pic, fee_ledger_id, caller);
    let wrap_before = ledger_balance_of(&pic, fee_ledger_id, wrap_id);
    let wrap_out = pic
        .update_call(
            wrap_id,
            caller,
            "submit_wrap_request",
            Encode!(&SubmitWrapRequestArgs {
                asset_id: fee_ledger_id,
                amount_e8s: Nat::from(WRAP_AMOUNT_E8S),
                evm_recipient: caller_evm.to_vec(),
                evm_nonce: 0,
                gas_limit: TEST_WRAP_GAS_LIMIT,
            })
            .expect("encode wrap submit"),
        )
        .unwrap();
    let wrap_result: Result<SubmitWrapRequestOk, ApiError> =
        Decode!(&wrap_out, Result<SubmitWrapRequestOk, ApiError>).expect("decode wrap submit");
    let wrap_ok = wrap_result.expect("wrap submit should accept and queue");

    pic.reinstall_canister(
        gateway_id,
        read_wasm(mock_wrap_wasm_path()),
        Encode!(&()).expect("encode mock wrap init"),
        Some(caller),
    )
    .unwrap_or_else(|err| panic!("reinstall gateway failed: {err}"));

    let failed = wait_for_wrap_status(
        &pic,
        wrap_id,
        &wrap_ok.request_id,
        WrapRequestStatus::Failed,
    );
    assert!(
        failed.pull_ledger_tx_id.is_some(),
        "pull should complete before recover"
    );
    assert!(failed.mint_tx_id.is_none(), "mint tx must be absent");

    let caller_mid = ledger_balance_of(&pic, fee_ledger_id, caller);
    let wrap_mid = ledger_balance_of(&pic, fee_ledger_id, wrap_id);
    assert!(
        caller_mid < caller_before,
        "caller balance should decrease after pull"
    );
    assert!(
        wrap_mid > wrap_before,
        "wrap balance should increase after pull"
    );

    let recover_arg = Encode!(&RecoverFailedWrapArgs {
        request_id: wrap_ok.request_id.clone(),
    })
    .expect("encode recover arg");
    let first_call = pic
        .submit_call(wrap_id, caller, "recover_failed_wrap", recover_arg.clone())
        .unwrap_or_else(|err| panic!("submit first recover failed: {err}"));
    let second_call = pic
        .submit_call(wrap_id, caller, "recover_failed_wrap", recover_arg)
        .unwrap_or_else(|err| panic!("submit second recover failed: {err}"));

    let first_out = pic
        .await_call(first_call)
        .unwrap_or_else(|err| panic!("await first recover failed: {err:?}"));
    let second_out = pic
        .await_call(second_call)
        .unwrap_or_else(|err| panic!("await second recover failed: {err:?}"));

    let first_result: Result<RequestOverview, ApiError> =
        Decode!(&first_out, Result<RequestOverview, ApiError>).expect("decode first recover");
    let second_result: Result<RequestOverview, ApiError> =
        Decode!(&second_out, Result<RequestOverview, ApiError>).expect("decode second recover");
    let (successes, failures): (Vec<_>, Vec<_>) = [first_result, second_result]
        .into_iter()
        .partition(Result::is_ok);

    assert_eq!(successes.len(), 1, "exactly one recover should succeed");
    assert_eq!(failures.len(), 1, "exactly one recover should fail");
    let recovered = successes
        .into_iter()
        .next()
        .expect("missing successful recover")
        .expect("successful recover payload");
    assert_eq!(recovered.status, WrapRequestStatus::Failed);
    assert!(
        recovered.withdraw_ledger_tx_id.is_some(),
        "successful recover should record withdraw tx"
    );
    let second_err = failures
        .into_iter()
        .next()
        .expect("missing failed recover")
        .expect_err("failed recover payload");
    match second_err {
        ApiError::InvalidArgument(detail) => {
            assert_eq!(detail.code, "withdraw.in_progress");
        }
        other => panic!("unexpected concurrent recover error: {other:?}"),
    }

    let caller_after = ledger_balance_of(&pic, fee_ledger_id, caller);
    let wrap_after = ledger_balance_of(&pic, fee_ledger_id, wrap_id);
    assert_eq!(
        caller_after - caller_mid,
        WRAP_AMOUNT_E8S,
        "caller should receive exactly one refund"
    );
    assert_eq!(
        wrap_mid - wrap_after,
        WRAP_AMOUNT_E8S + 10,
        "wrap canister should pay one refund plus the ledger transfer fee"
    );

    let overview = wrap_get_request(&pic, wrap_id, &wrap_ok.request_id).expect("final overview");
    assert_eq!(overview.status, WrapRequestStatus::Failed);
    assert!(overview.withdraw_ledger_tx_id.is_some());
}
