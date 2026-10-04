//! Replicated query dispatch is separate from the read-only composite query path.
use super::*;
use evm_db::chain_data::query_tx::QUERY_TX_TIMEOUT_SECONDS;
use evm_db::chain_data::{QueryTxPhase, QueryTxSession};

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct PendingQueryTxView {
    pub attempt_id: u64,
    pub tx_id: Vec<u8>,
    pub target: Option<Principal>,
    pub method: String,
    pub phase: QueryTxPhase,
    pub deadline: u64,
    pub error: Option<String>,
}
#[ic_cdk::query]
fn get_pending_query_tx() -> Option<PendingQueryTxView> {
    chain::pending_query_tx().map(|s| PendingQueryTxView {
        attempt_id: s.attempt_id,
        tx_id: s.tx_id.to_vec(),
        target: (!s.target.is_empty()).then(|| Principal::from_slice(&s.target)),
        method: s.method,
        phase: s.phase,
        deadline: s.deadline,
        error: s.reply.and_then(Result::err),
    })
}
#[ic_cdk::query]
fn get_tx_query_precompile_allowlist() -> Vec<PrecompileAllowedView> {
    with_state(|s| {
        s.tx_query_precompile_allowlist
            .iter()
            .filter_map(|e| decode_precompile_allow_key_for_principal(e.key()))
            .collect()
    })
}
#[ic_cdk::update]
fn add_tx_query_precompile_allowed_method(args: PrecompileAllowArgs) -> Result<(), String> {
    register_tx_query_method(args, true)
}
#[ic_cdk::update]
fn remove_tx_query_precompile_allowed_method(args: PrecompileAllowArgs) -> Result<(), String> {
    register_tx_query_method(args, false)
}
fn register_tx_query_method(args: PrecompileAllowArgs, add: bool) -> Result<(), String> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(reason);
    }
    require_execution_config_write()?;
    validate_query_precompile_allow_args(&args)?;
    let key = precompile_allow_key_for_principal(args.target, &args.method);
    with_state_mut(|s| {
        if add {
            s.tx_query_precompile_allowlist.insert(key, 1);
        } else {
            s.tx_query_precompile_allowlist.remove(&key);
        }
    });
    Ok(())
}
pub(super) fn require_execution_config_write() -> Result<(), String> {
    require_control_plane_write()?;
    chain::require_no_pending_query_tx().map_err(|_| "ic_query.tx_busy".into())
}
pub(super) fn schedule_query_dispatch() {
    let Some(job) = chain::pending_query_tx() else {
        return;
    };
    if job.phase != QueryTxPhase::Waiting {
        return;
    }
    let attempt_id = job.attempt_id;
    let remaining = job.deadline.saturating_sub(current_time_nanos());
    ic_cdk_timers::set_timer(std::time::Duration::from_nanos(remaining), async move {
        if chain::pending_query_tx().is_some_and(|j| j.attempt_id == attempt_id) {
            chain::expire_query_tx(current_time_nanos());
            repair_query_mining_schedule();
            schedule_mining();
        }
    });
    ic_cdk_timers::set_timer(std::time::Duration::ZERO, async move {
        dispatch_query_tx(job).await;
    });
}
async fn dispatch_query_tx(job: QueryTxSession) {
    if reject_write_reason().is_some() {
        return;
    }
    if chain::expire_query_tx(current_time_nanos()) {
        schedule_mining();
        return;
    }
    if !chain::start_query_tx_call(job.attempt_id) {
        return;
    }
    let allowed = with_state(|s| {
        s.tx_query_precompile_allowlist
            .get(&precompile_allow_key(&job.target, &job.method))
            .is_some()
    });
    let reply = if allowed {
        Call::bounded_wait(Principal::from_slice(&job.target), &job.method)
            .take_raw_args(job.arg)
            .change_timeout(QUERY_TX_TIMEOUT_SECONDS)
            .await
            .map(|reply| reply.into_bytes())
            .map_err(|_| "ic_query.call_failed".to_string())
    } else {
        Err("ic_query.tx_allowlist_miss".into())
    };
    if chain::finish_query_tx_call(job.attempt_id, current_time_nanos(), reply) {
        schedule_mining();
    }
}

#[ic_cdk::query(composite = true)]
async fn rpc_eth_estimate_gas_object_at_with_query_precompile(
    call: RpcCallObjectView,
    tag: RpcBlockTagView,
) -> Result<u64, RpcErrorView> {
    ic_evm_rpc::rpc_eth_estimate_gas_object_at_async(call, tag, resolve_tx_query_for_estimation)
        .await
}

async fn resolve_tx_query_for_estimation(
    request: evm_core::kasane_precompiles::IcpQueryRequest,
) -> Result<Vec<u8>, String> {
    if !with_state(|s| {
        s.tx_query_precompile_allowlist
            .get(&precompile_allow_key(&request.target, &request.method))
            .is_some()
    }) {
        return Err("ic_query.tx_allowlist_miss".into());
    }
    let bytes = Call::bounded_wait(Principal::from_slice(&request.target), &request.method)
        .take_raw_args(request.arg)
        .change_timeout(QUERY_TX_TIMEOUT_SECONDS)
        .await
        .map_err(|_| "ic_query.call_failed".to_string())?
        .into_bytes();
    if bytes.len() > MAX_RETURN_DATA {
        return Err("ic_query.response_too_large".into());
    }
    Ok(bytes)
}
