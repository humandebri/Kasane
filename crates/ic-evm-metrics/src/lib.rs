//! どこで: canister の運用メトリクス出力層 / 何を: Prometheusエンコード / なぜ: wrapper本体から責務分離するため

use ic_evm_rpc_types::DropCountView;
use ic_metrics_encoder::MetricsEncoder;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrometheusSnapshot {
    pub cycles_balance: u128,
    pub stable_memory_pages: u64,
    pub heap_memory_pages: u64,
    pub tip_block_number: u64,
    pub queue_len: u64,
    pub total_submitted: u64,
    pub total_included: u64,
    pub total_dropped: u64,
    pub auto_mine_enabled: bool,
    pub is_producing: bool,
    pub mining_scheduled: bool,
    pub mining_interval_ms: u64,
    pub last_block_time: u64,
    pub pruned_before_block: Option<u64>,
    pub drop_counts: Vec<DropCountView>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrometheusSnapshotInput {
    pub cycles_balance: u128,
    pub stable_memory_pages: u64,
    pub heap_memory_pages: u64,
    pub tip_block_number: u64,
    pub queue_len: u64,
    pub total_submitted: u64,
    pub total_included: u64,
    pub total_dropped: u64,
    pub auto_mine_enabled: bool,
    pub is_producing: bool,
    pub mining_scheduled: bool,
    pub mining_interval_ms: u64,
    pub last_block_time: u64,
    pub pruned_before_block: Option<u64>,
    pub drop_counts_by_code: Vec<u64>,
}

pub fn build_prometheus_snapshot(input: PrometheusSnapshotInput) -> PrometheusSnapshot {
    let mut drop_counts = Vec::new();
    for (idx, count) in input.drop_counts_by_code.iter().enumerate() {
        if *count == 0 {
            continue;
        }
        if let Ok(code) = u16::try_from(idx) {
            drop_counts.push(DropCountView {
                code,
                count: *count,
            });
        }
    }

    PrometheusSnapshot {
        cycles_balance: input.cycles_balance,
        stable_memory_pages: input.stable_memory_pages,
        heap_memory_pages: input.heap_memory_pages,
        tip_block_number: input.tip_block_number,
        queue_len: input.queue_len,
        total_submitted: input.total_submitted,
        total_included: input.total_included,
        total_dropped: input.total_dropped,
        auto_mine_enabled: input.auto_mine_enabled,
        is_producing: input.is_producing,
        mining_scheduled: input.mining_scheduled,
        mining_interval_ms: input.mining_interval_ms,
        last_block_time: input.last_block_time,
        pruned_before_block: input.pruned_before_block,
        drop_counts,
    }
}

pub fn encode_prometheus(now_nanos: u64, snapshot: &PrometheusSnapshot) -> Result<String, String> {
    let ts_millis_u64 = now_nanos / 1_000_000;
    let ts_millis = i64::try_from(ts_millis_u64).unwrap_or(i64::MAX);
    let mut encoder = MetricsEncoder::new(Vec::new(), ts_millis);

    encoder
        .encode_gauge(
            "ic_evm_cycles_balance",
            to_f64_saturating(snapshot.cycles_balance),
            "Canister cycle balance.",
        )
        .map_err(map_io)?;
    encoder
        .encode_gauge(
            "ic_evm_stable_memory_pages",
            to_f64_u64(snapshot.stable_memory_pages),
            "Stable memory pages in use.",
        )
        .map_err(map_io)?;
    encoder
        .encode_gauge(
            "ic_evm_heap_memory_pages",
            to_f64_u64(snapshot.heap_memory_pages),
            "Wasm heap memory pages in use.",
        )
        .map_err(map_io)?;
    encoder
        .encode_gauge(
            "ic_evm_tip_block_number",
            to_f64_u64(snapshot.tip_block_number),
            "Latest produced block number.",
        )
        .map_err(map_io)?;
    encoder
        .encode_gauge(
            "ic_evm_queue_len",
            to_f64_u64(snapshot.queue_len),
            "Pending transaction queue length.",
        )
        .map_err(map_io)?;
    encoder
        .encode_counter(
            "ic_evm_total_submitted",
            to_f64_u64(snapshot.total_submitted),
            "Total submitted transactions.",
        )
        .map_err(map_io)?;
    encoder
        .encode_counter(
            "ic_evm_total_included",
            to_f64_u64(snapshot.total_included),
            "Total included transactions.",
        )
        .map_err(map_io)?;
    encoder
        .encode_counter(
            "ic_evm_total_dropped",
            to_f64_u64(snapshot.total_dropped),
            "Total dropped transactions.",
        )
        .map_err(map_io)?;
    encoder
        .encode_gauge(
            "ic_evm_auto_mine_enabled",
            bool_to_gauge(snapshot.auto_mine_enabled),
            "1 when auto-mining is enabled.",
        )
        .map_err(map_io)?;
    encoder
        .encode_gauge(
            "ic_evm_is_producing",
            bool_to_gauge(snapshot.is_producing),
            "1 when a block production call is in progress.",
        )
        .map_err(map_io)?;
    encoder
        .encode_gauge(
            "ic_evm_mining_scheduled",
            bool_to_gauge(snapshot.mining_scheduled),
            "1 when timer-driven mining is scheduled.",
        )
        .map_err(map_io)?;
    encoder
        .encode_gauge(
            "ic_evm_mining_interval_ms",
            to_f64_u64(snapshot.mining_interval_ms),
            "Configured auto-mining interval in milliseconds.",
        )
        .map_err(map_io)?;
    encoder
        .encode_gauge(
            "ic_evm_last_block_time_seconds",
            to_f64_u64(snapshot.last_block_time),
            "Timestamp of the last produced block.",
        )
        .map_err(map_io)?;
    encoder
        .encode_gauge(
            "ic_evm_pruned_before_block_present",
            bool_to_gauge(snapshot.pruned_before_block.is_some()),
            "1 when prune boundary is set.",
        )
        .map_err(map_io)?;
    encoder
        .encode_gauge(
            "ic_evm_pruned_before_block",
            to_f64_u64(snapshot.pruned_before_block.unwrap_or(0)),
            "Prune boundary block number (0 when unset).",
        )
        .map_err(map_io)?;

    let mut drop_builder = encoder
        .counter_vec(
            "ic_evm_drop_count_total",
            "Dropped transaction counts by drop code.",
        )
        .map_err(map_io)?;
    for sample in &snapshot.drop_counts {
        let code_text = sample.code.to_string();
        drop_builder = drop_builder
            .value(&[("code", code_text.as_str())], to_f64_u64(sample.count))
            .map_err(map_io)?;
    }

    String::from_utf8(encoder.into_inner()).map_err(|err| format!("metrics.encode.utf8: {err}"))
}

fn to_f64_saturating(value: u128) -> f64 {
    match u64::try_from(value) {
        Ok(v) => to_f64_u64(v),
        Err(_) => to_f64_u64(u64::MAX),
    }
}

fn to_f64_u64(value: u64) -> f64 {
    value as f64
}

fn bool_to_gauge(value: bool) -> f64 {
    if value {
        1.0
    } else {
        0.0
    }
}

fn map_io(err: std::io::Error) -> String {
    format!("metrics.encode.io: {err}")
}

#[cfg(test)]
mod tests {
    use super::{
        build_prometheus_snapshot, encode_prometheus, PrometheusSnapshot, PrometheusSnapshotInput,
    };
    use ic_evm_rpc_types::DropCountView;

    #[test]
    fn encode_prometheus_includes_expected_metrics() {
        let snapshot = PrometheusSnapshot {
            cycles_balance: 1234,
            stable_memory_pages: 11,
            heap_memory_pages: 22,
            tip_block_number: 7,
            queue_len: 3,
            total_submitted: 5,
            total_included: 4,
            total_dropped: 1,
            auto_mine_enabled: true,
            is_producing: false,
            mining_scheduled: true,
            mining_interval_ms: 5000,
            last_block_time: 999,
            pruned_before_block: Some(6),
            drop_counts: vec![DropCountView { code: 2, count: 9 }],
        };
        let text = encode_prometheus(1_700_000_000_000_000_000, &snapshot)
            .expect("encoding should succeed");
        assert!(text.contains("ic_evm_cycles_balance"));
        assert!(text.contains("ic_evm_total_submitted"));
        assert!(text.contains("ic_evm_drop_count_total{code=\"2\"} 9"));
    }

    #[test]
    fn build_snapshot_filters_zero_and_maps_code_index() {
        let snapshot = build_prometheus_snapshot(PrometheusSnapshotInput {
            cycles_balance: 1,
            stable_memory_pages: 1,
            heap_memory_pages: 1,
            tip_block_number: 1,
            queue_len: 1,
            total_submitted: 1,
            total_included: 1,
            total_dropped: 1,
            auto_mine_enabled: false,
            is_producing: false,
            mining_scheduled: false,
            mining_interval_ms: 1,
            last_block_time: 1,
            pruned_before_block: None,
            drop_counts_by_code: vec![0, 3, 0, 4],
        });
        assert_eq!(
            snapshot.drop_counts,
            vec![
                DropCountView { code: 1, count: 3 },
                DropCountView { code: 3, count: 4 }
            ]
        );
    }
}
