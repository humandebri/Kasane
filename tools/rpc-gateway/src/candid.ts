// どこで: Gateway Candid定義 / 何を: 必要RPCメソッドをIDL化 / なぜ: Actor境界で型不一致を防ぐため

import type { IDL } from "@dfinity/candid";

export const idlFactory: IDL.InterfaceFactory = ({ IDL }) => {
  const RpcAccessListItemView = IDL.Record({
    address: IDL.Vec(IDL.Nat8),
    storage_keys: IDL.Vec(IDL.Vec(IDL.Nat8)),
  });
  const RpcCallObjectView = IDL.Record({
    to: IDL.Opt(IDL.Vec(IDL.Nat8)),
    from: IDL.Opt(IDL.Vec(IDL.Nat8)),
    gas: IDL.Opt(IDL.Nat64),
    gas_price: IDL.Opt(IDL.Nat),
    nonce: IDL.Opt(IDL.Nat64),
    max_fee_per_gas: IDL.Opt(IDL.Nat),
    max_priority_fee_per_gas: IDL.Opt(IDL.Nat),
    chain_id: IDL.Opt(IDL.Nat64),
    tx_type: IDL.Opt(IDL.Nat64),
    access_list: IDL.Opt(IDL.Vec(RpcAccessListItemView)),
    value: IDL.Opt(IDL.Vec(IDL.Nat8)),
    data: IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const RpcCallResultView = IDL.Record({
    status: IDL.Nat8,
    gas_used: IDL.Nat64,
    return_data: IDL.Vec(IDL.Nat8),
    revert_data: IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const RpcErrorView = IDL.Record({
    code: IDL.Nat32,
    message: IDL.Text,
  });
  const DecodedTxView = IDL.Record({
    to: IDL.Opt(IDL.Vec(IDL.Nat8)),
    value: IDL.Vec(IDL.Nat8),
    from: IDL.Vec(IDL.Nat8),
    chain_id: IDL.Opt(IDL.Nat64),
    nonce: IDL.Nat64,
    gas_limit: IDL.Nat64,
    input: IDL.Vec(IDL.Nat8),
    gas_price: IDL.Opt(IDL.Nat),
    max_fee_per_gas: IDL.Opt(IDL.Nat),
    max_priority_fee_per_gas: IDL.Opt(IDL.Nat),
  });
  const EthTxView = IDL.Record({
    raw: IDL.Vec(IDL.Nat8),
    tx_index: IDL.Opt(IDL.Nat32),
    decode_ok: IDL.Bool,
    hash: IDL.Vec(IDL.Nat8),
    kind: IDL.Variant({ EthSigned: IDL.Null, IcSynthetic: IDL.Null }),
    block_number: IDL.Opt(IDL.Nat64),
    eth_tx_hash: IDL.Opt(IDL.Vec(IDL.Nat8)),
    decoded: IDL.Opt(DecodedTxView),
  });
  const EthReceiptView = IDL.Record({
    effective_gas_price: IDL.Nat64,
    status: IDL.Nat8,
    l1_data_fee: IDL.Nat,
    tx_index: IDL.Nat32,
    logs: IDL.Vec(
      IDL.Record({
        log_index: IDL.Nat32,
        data: IDL.Vec(IDL.Nat8),
        topics: IDL.Vec(IDL.Vec(IDL.Nat8)),
        address: IDL.Vec(IDL.Nat8),
      })
    ),
    total_fee: IDL.Nat,
    block_number: IDL.Nat64,
    operator_fee: IDL.Nat,
    eth_tx_hash: IDL.Opt(IDL.Vec(IDL.Nat8)),
    gas_used: IDL.Nat64,
    contract_address: IDL.Opt(IDL.Vec(IDL.Nat8)),
    tx_hash: IDL.Vec(IDL.Nat8),
  });
  const EthBlockView = IDL.Record({
    txs: IDL.Variant({ Full: IDL.Vec(EthTxView), Hashes: IDL.Vec(IDL.Vec(IDL.Nat8)) }),
    block_hash: IDL.Vec(IDL.Nat8),
    number: IDL.Nat64,
    timestamp: IDL.Nat64,
    beneficiary: IDL.Vec(IDL.Nat8),
    state_root: IDL.Vec(IDL.Nat8),
    parent_hash: IDL.Vec(IDL.Nat8),
    base_fee_per_gas: IDL.Opt(IDL.Nat64),
    gas_limit: IDL.Opt(IDL.Nat64),
    gas_used: IDL.Opt(IDL.Nat64),
  });
  const RpcBlockLookupView = IDL.Variant({
    NotFound: IDL.Null,
    Found: EthBlockView,
    Pruned: IDL.Record({ pruned_before_block: IDL.Nat64 }),
  });
  const EthLogsCursorView = IDL.Record({
    tx_index: IDL.Nat32,
    log_index: IDL.Nat32,
    block_number: IDL.Nat64,
  });
  const EthLogItemView = IDL.Record({
    tx_index: IDL.Nat32,
    log_index: IDL.Nat32,
    data: IDL.Vec(IDL.Nat8),
    block_number: IDL.Nat64,
    topics: IDL.Vec(IDL.Vec(IDL.Nat8)),
    address: IDL.Vec(IDL.Nat8),
    eth_tx_hash: IDL.Opt(IDL.Vec(IDL.Nat8)),
    tx_hash: IDL.Vec(IDL.Nat8),
  });
  const EthLogsPageView = IDL.Record({
    next_cursor: IDL.Opt(EthLogsCursorView),
    items: IDL.Vec(EthLogItemView),
  });
  const EthLogFilterView = IDL.Record({
    limit: IDL.Opt(IDL.Nat32),
    topic0: IDL.Opt(IDL.Vec(IDL.Nat8)),
    topic1: IDL.Opt(IDL.Vec(IDL.Nat8)),
    address: IDL.Opt(IDL.Vec(IDL.Nat8)),
    to_block: IDL.Opt(IDL.Nat64),
    from_block: IDL.Opt(IDL.Nat64),
  });
  const GetLogsErrorView = IDL.Variant({
    TooManyResults: IDL.Null,
    RangeTooLarge: IDL.Null,
    InvalidArgument: IDL.Text,
    UnsupportedFilter: IDL.Text,
  });
  const RpcReceiptLookupView = IDL.Variant({
    NotFound: IDL.Null,
    Found: EthReceiptView,
    PossiblyPruned: IDL.Record({ pruned_before_block: IDL.Nat64 }),
    Pruned: IDL.Record({ pruned_before_block: IDL.Nat64 }),
  });
  const SubmitTxError = IDL.Variant({
    Internal: IDL.Text,
    Rejected: IDL.Text,
    InvalidArgument: IDL.Text,
  });
  const OpsModeView = IDL.Variant({
    Low: IDL.Null,
    Normal: IDL.Null,
    Critical: IDL.Null,
  });
  const OpsConfigView = IDL.Record({
    low_watermark: IDL.Nat,
    freeze_on_critical: IDL.Bool,
    critical: IDL.Nat,
  });
  const OpsStatusView = IDL.Record({
    needs_migration: IDL.Bool,
    critical_corrupt: IDL.Bool,
    decode_failure_last_ts: IDL.Nat64,
    log_filter_override: IDL.Opt(IDL.Text),
    last_cycle_balance: IDL.Nat,
    mode: OpsModeView,
    instruction_soft_limit: IDL.Nat64,
    last_check_ts: IDL.Nat64,
    mining_error_count: IDL.Nat64,
    log_truncated_count: IDL.Nat64,
    schema_version: IDL.Nat32,
    safe_stop_latched: IDL.Bool,
    decode_failure_last_label: IDL.Opt(IDL.Text),
    prune_error_count: IDL.Nat64,
    block_gas_limit: IDL.Nat64,
    config: OpsConfigView,
    decode_failure_count: IDL.Nat64,
  });

  return IDL.Service({
    expected_nonce_by_address: IDL.Func(
      [IDL.Vec(IDL.Nat8)],
      [IDL.Variant({ Ok: IDL.Nat64, Err: IDL.Text })],
      ["query"]
    ),
    rpc_eth_chain_id: IDL.Func([], [IDL.Nat64], ["query"]),
    rpc_eth_block_number: IDL.Func([], [IDL.Nat64], ["query"]),
    rpc_eth_get_block_by_number: IDL.Func([IDL.Nat64, IDL.Bool], [IDL.Opt(EthBlockView)], ["query"]),
    rpc_eth_get_block_by_number_with_status: IDL.Func([IDL.Nat64, IDL.Bool], [RpcBlockLookupView], ["query"]),
    rpc_eth_get_block_number_by_hash: IDL.Func(
      [IDL.Vec(IDL.Nat8), IDL.Nat32],
      [IDL.Variant({ Ok: IDL.Opt(IDL.Nat64), Err: IDL.Text })],
      ["query"]
    ),
    rpc_eth_get_transaction_by_eth_hash: IDL.Func([IDL.Vec(IDL.Nat8)], [IDL.Opt(EthTxView)], ["query"]),
    rpc_eth_get_transaction_by_tx_id: IDL.Func([IDL.Vec(IDL.Nat8)], [IDL.Opt(EthTxView)], ["query"]),
    rpc_eth_get_transaction_receipt_by_eth_hash: IDL.Func([IDL.Vec(IDL.Nat8)], [IDL.Opt(EthReceiptView)], ["query"]),
    rpc_eth_get_transaction_receipt_with_status: IDL.Func([IDL.Vec(IDL.Nat8)], [RpcReceiptLookupView], ["query"]),
    rpc_eth_get_logs_paged: IDL.Func(
      [EthLogFilterView, IDL.Opt(EthLogsCursorView), IDL.Nat32],
      [IDL.Variant({ Ok: EthLogsPageView, Err: GetLogsErrorView })],
      ["query"]
    ),
    rpc_eth_get_balance: IDL.Func([IDL.Vec(IDL.Nat8)], [IDL.Variant({ Ok: IDL.Vec(IDL.Nat8), Err: IDL.Text })], ["query"]),
    rpc_eth_get_code: IDL.Func([IDL.Vec(IDL.Nat8)], [IDL.Variant({ Ok: IDL.Vec(IDL.Nat8), Err: IDL.Text })], ["query"]),
    rpc_eth_get_storage_at: IDL.Func(
      [IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8)],
      [IDL.Variant({ Ok: IDL.Vec(IDL.Nat8), Err: IDL.Text })],
      ["query"]
    ),
    rpc_eth_call_object: IDL.Func(
      [RpcCallObjectView],
      [IDL.Variant({ Ok: RpcCallResultView, Err: RpcErrorView })],
      ["query"]
    ),
    rpc_eth_estimate_gas_object: IDL.Func(
      [RpcCallObjectView],
      [IDL.Variant({ Ok: IDL.Nat64, Err: RpcErrorView })],
      ["query"]
    ),
    rpc_eth_call_rawtx: IDL.Func([IDL.Vec(IDL.Nat8)], [IDL.Variant({ Ok: IDL.Vec(IDL.Nat8), Err: IDL.Text })], ["query"]),
    rpc_eth_send_raw_transaction: IDL.Func(
      [IDL.Vec(IDL.Nat8)],
      [IDL.Variant({ Ok: IDL.Vec(IDL.Nat8), Err: SubmitTxError })],
      []
    ),
    get_ops_status: IDL.Func([], [OpsStatusView], ["query"]),
  });
};
