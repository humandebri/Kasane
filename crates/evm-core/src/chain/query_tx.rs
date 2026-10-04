//! Reserve before yielding; provisional state is always discarded and replayed.
use super::*;
use crate::kasane_precompiles::{with_icp_query_reply, IcpQueryReply};
use evm_db::chain_data::query_tx::QUERY_TX_TIMEOUT_NANOS;
use evm_db::chain_data::{QueryTxPhase, QueryTxSession};

pub fn pending_query_tx() -> Option<QueryTxSession> {
    with_state(|s| s.query_tx_state.get().session.clone())
}
pub fn require_no_pending_query_tx() -> Result<(), ChainError> {
    if pending_query_tx().is_some() {
        Err(ChainError::QueryTxBusy)
    } else {
        Ok(())
    }
}
pub(super) fn query_tx_snapshot() -> [u8; 32] {
    with_state(|s| {
        let c = s.chain_state.get();
        let h = s.head.get();
        let mut bytes = Vec::new();
        for v in [
            h.number,
            h.timestamp,
            *s.evm_state_epoch.get(),
            c.base_fee,
            c.block_gas_limit,
            c.min_gas_price,
            c.min_priority_fee,
            c.update_instruction_soft_limit,
        ] {
            bytes.extend_from_slice(&v.to_be_bytes());
        }
        bytes.extend_from_slice(&h.block_hash);
        bytes.extend_from_slice(&s.runtime_config.get().to_bytes());
        bytes.extend_from_slice(&s.wrap_evm_config.get().to_bytes());
        for map in [
            &s.tx_query_precompile_allowlist,
            &s.icp_update_precompile_allowlist,
            &s.wrap_allowed_assets,
        ] {
            bytes.extend_from_slice(&map.len().to_be_bytes());
            for e in map.iter() {
                bytes.extend_from_slice(&(e.key().len() as u64).to_be_bytes());
                bytes.extend_from_slice(e.key());
                bytes.push(e.value());
            }
        }
        hash::keccak256(&bytes)
    })
}
pub(super) fn reserve_query_tx(
    tx_id: TxId,
    request: Option<IcpQueryRequest>,
    ctx: &BlockExecContext,
) -> Result<(), ChainError> {
    let snapshot = query_tx_snapshot();
    let now = crate::time::now_ns();
    with_state_mut(|s| {
        let mut state = s.query_tx_state.get().clone();
        let attempt_id = if let Some(old) = state.session.as_ref() {
            if old.phase != QueryTxPhase::Reserved || old.tx_id != tx_id.0 {
                return Err(ChainError::QueryTxBusy);
            }
            old.attempt_id
        } else {
            let id = state.next_attempt_id;
            state.next_attempt_id = id
                .checked_add(1)
                .ok_or_else(|| ChainError::InvariantViolation("query.attempt_overflow".into()))?;
            id
        };
        let phase = if request.is_some() {
            QueryTxPhase::Waiting
        } else {
            QueryTxPhase::Reserved
        };
        let request = request.unwrap_or(IcpQueryRequest {
            target: Vec::new(),
            method: String::new(),
            arg: Vec::new(),
        });
        state.session = Some(QueryTxSession {
            attempt_id,
            tx_id: tx_id.0,
            phase,
            block_number: ctx.block_number,
            timestamp: ctx.timestamp,
            base_fee: ctx.base_fee,
            block_gas_limit: ctx.block_gas_limit,
            snapshot,
            update_active_count: *s.icp_update_active_count.get(),
            target: request.target,
            method: request.method,
            arg: request.arg,
            started_at: now,
            deadline: now.saturating_add(QUERY_TX_TIMEOUT_NANOS),
            reply: None,
        });
        s.query_tx_state.set(state);
        Ok(())
    })
}
/// Move to Calling in the same message that dispatches the call; never send twice.
pub fn start_query_tx_call(attempt_id: u64) -> bool {
    with_state_mut(|s| {
        let mut state = s.query_tx_state.get().clone();
        let Some(job) = state.session.as_mut() else {
            return false;
        };
        if job.attempt_id != attempt_id || job.phase != QueryTxPhase::Waiting {
            return false;
        }
        job.phase = QueryTxPhase::Calling;
        s.query_tx_state.set(state);
        true
    })
}
pub fn finish_query_tx_call(attempt_id: u64, now: u64, reply: Result<Vec<u8>, String>) -> bool {
    with_state_mut(|s| {
        let mut state = s.query_tx_state.get().clone();
        let Some(job) = state.session.as_mut() else {
            return false;
        };
        if !verified_core::kasane_precompiles::query_tx_callback_allowed(
            job.attempt_id,
            attempt_id,
            job.phase == QueryTxPhase::Calling,
        ) {
            return false;
        }
        job.reply = Some(if now >= job.deadline {
            Err("ic_query.timeout".into())
        } else {
            match reply {
                Ok(v) if v.len() > evm_db::chain_data::constants::MAX_RETURN_DATA => {
                    Err("ic_query.response_too_large".into())
                }
                Ok(v) => Ok(v),
                Err(mut e) => {
                    if e.len() > 256 {
                        let mut end = 256;
                        while !e.is_char_boundary(end) {
                            end -= 1;
                        }
                        e.truncate(end);
                    }
                    Err(e)
                }
            }
        });
        job.phase = QueryTxPhase::Ready;
        s.query_tx_state.set(state);
        true
    })
}
pub fn expire_query_tx(now: u64) -> bool {
    let Some(job) = pending_query_tx() else {
        return false;
    };
    if job.phase == QueryTxPhase::Ready
        && now >= job.deadline
        && job.reply.as_ref().is_some_and(Result::is_ok)
    {
        with_state_mut(|s| {
            let mut state = s.query_tx_state.get().clone();
            if let Some(j) = state.session.as_mut() {
                j.reply = Some(Err("ic_query.timeout".into()));
            }
            s.query_tx_state.set(state);
        });
        return true;
    }
    if matches!(job.phase, QueryTxPhase::Waiting | QueryTxPhase::Calling) && now >= job.deadline {
        if job.phase == QueryTxPhase::Waiting {
            start_query_tx_call(job.attempt_id);
        }
        return finish_query_tx_call(job.attempt_id, now, Err("ic_query.timeout".into()));
    }
    false
}
pub fn interrupt_query_tx_after_upgrade() {
    if let Some(job) = pending_query_tx().filter(|j| j.phase == QueryTxPhase::Reserved) {
        drop_stale_query_tx(&job);
        return;
    }
    with_state_mut(|s| {
        let mut state = s.query_tx_state.get().clone();
        if let Some(job) = state.session.as_mut() {
            job.reply = Some(Err("ic_query.interrupted".into()));
            job.phase = QueryTxPhase::Ready;
            s.query_tx_state.set(state);
        }
    });
}
pub(super) fn clear_query_tx(state: &mut StableState) {
    let mut session = state.query_tx_state.get().clone();
    session.session = None;
    state.query_tx_state.set(session);
}
fn drop_stale_query_tx(job: &QueryTxSession) {
    with_state_mut(|s| {
        let tx_id = TxId(job.tx_id);
        advance_sender_after_tx(s, tx_id, None, None, false);
        mark_dropped_and_purge_payload(
            s,
            tx_id,
            evm_db::chain_data::constants::DROP_CODE_QUERY_INTERRUPTED,
        );
        // Keep the existing fixed stable metrics layout; the tx location carries the specific reason.
        let mut metrics = *s.metrics_state.get();
        metrics.record_drop(DROP_CODE_EXEC, 1);
        s.metrics_state.set(metrics);
        clear_query_tx(s);
    });
}
pub fn produce_block(max_txs: usize) -> Result<ProduceBlockOutcome, ChainError> {
    if !verified_core::block::valid_block_limit(max_txs) {
        return Err(ChainError::InvalidLimit);
    }
    if with_state(|s| s.query_tx_state.get().session.is_none()) {
        return produce_block_inner(max_txs, None);
    }
    produce_query_block()
}

// Keep the owned session and its reply buffers off the ordinary block path.
#[inline(never)]
fn produce_query_block() -> Result<ProduceBlockOutcome, ChainError> {
    let job = pending_query_tx().expect("pending query transaction");
    match job.phase {
        QueryTxPhase::Waiting | QueryTxPhase::Calling => Err(ChainError::QueryTxBusy),
        QueryTxPhase::Reserved => {
            if query_tx_snapshot() != job.snapshot {
                drop_stale_query_tx(&job);
                return Err(ChainError::NoExecutableTx);
            }
            produce_block_inner(1, Some(&job))
        }
        QueryTxPhase::Ready => {
            let transaction_matches =
                with_state(|s| s.pending_meta_by_tx_id.get(&TxId(job.tx_id)).is_some());
            if !verified_core::kasane_precompiles::query_tx_commit_allowed(
                job.phase == QueryTxPhase::Ready,
                query_tx_snapshot() == job.snapshot,
                transaction_matches,
            ) {
                drop_stale_query_tx(&job);
                return Err(ChainError::NoExecutableTx);
            }
            let request = IcpQueryRequest {
                target: job.target.clone(),
                method: job.method.clone(),
                arg: job.arg.clone(),
            };
            let reply = match job.reply.clone().expect("ready reply") {
                Ok(v) => IcpQueryReply::Ok(v),
                Err(e) => IcpQueryReply::Err(e),
            };
            let result =
                with_icp_query_reply(request, reply, || produce_block_inner(1, Some(&job)));
            if matches!(
                result,
                Err(ChainError::ExecFailed(Some(ExecError::SnapshotChanged)))
            ) {
                drop_stale_query_tx(&job);
            }
            result
        }
    }
}
