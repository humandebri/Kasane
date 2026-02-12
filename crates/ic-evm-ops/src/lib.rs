//! どこで: wrapper運用制御の内部ロジック層 / 何を: ops判定ロジック分離 / なぜ: canister本体の責務を薄くするため

use evm_db::chain_data::{OpsConfigV1, OpsMode, OpsStateV1};
use ic_evm_rpc_types::OpsModeView;

pub fn observe_cycles(
    balance: u128,
    now: u64,
    config: OpsConfigV1,
    mut ops: OpsStateV1,
) -> OpsStateV1 {
    let next_mode = if balance < config.critical {
        if config.freeze_on_critical {
            ops.safe_stop_latched = true;
        }
        OpsMode::Critical
    } else if ops.safe_stop_latched && config.freeze_on_critical && balance < config.low_watermark {
        OpsMode::Critical
    } else {
        if balance >= config.low_watermark {
            ops.safe_stop_latched = false;
        }
        if balance < config.low_watermark {
            OpsMode::Low
        } else {
            OpsMode::Normal
        }
    };

    ops.last_cycle_balance = balance;
    ops.last_check_ts = now;
    ops.mode = next_mode;
    ops
}

pub fn reject_write_reason(needs_migration: bool, mode: OpsMode) -> Option<String> {
    // データプレーン向けの拒否理由。
    // 制御プレーンの権限判定は wrapper 側で分離して扱う。
    if needs_migration {
        return Some("ops.write.needs_migration".to_string());
    }
    if mode == OpsMode::Critical {
        return Some("ops.write.cycle_critical".to_string());
    }
    None
}

pub fn reject_write_reason_with_mode_provider<F>(
    needs_migration: bool,
    mode_provider: F,
) -> Option<String>
where
    F: FnOnce() -> OpsMode,
{
    if needs_migration {
        return reject_write_reason(true, OpsMode::Normal);
    }
    reject_write_reason(false, mode_provider())
}

pub fn mode_to_view(mode: OpsMode) -> OpsModeView {
    match mode {
        OpsMode::Normal => OpsModeView::Normal,
        OpsMode::Low => OpsModeView::Low,
        OpsMode::Critical => OpsModeView::Critical,
    }
}

pub fn decode_failure_label_view(raw: [u8; 32]) -> Option<String> {
    let end = raw.iter().position(|b| *b == 0).unwrap_or(raw.len());
    if end == 0 {
        return None;
    }
    let bytes = &raw[..end];
    if bytes.iter().all(|b| {
        b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'.' || *b == b'_' || *b == b'-'
    }) {
        return Some(String::from_utf8_lossy(bytes).to_string());
    }
    let mut out = String::from("hex:");
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::{
        decode_failure_label_view, observe_cycles, reject_write_reason,
        reject_write_reason_with_mode_provider,
    };
    use evm_db::chain_data::{OpsConfigV1, OpsMode, OpsStateV1};

    #[test]
    fn observe_cycles_latches_on_critical() {
        let cfg = OpsConfigV1 {
            low_watermark: 200,
            critical: 100,
            freeze_on_critical: true,
        };
        let state = observe_cycles(50, 1, cfg, OpsStateV1::new());
        assert_eq!(state.mode, OpsMode::Critical);
        assert!(state.safe_stop_latched);
    }

    #[test]
    fn reject_reason_priority() {
        assert_eq!(
            reject_write_reason(true, OpsMode::Normal),
            Some("ops.write.needs_migration".to_string())
        );
        assert_eq!(
            reject_write_reason(false, OpsMode::Critical),
            Some("ops.write.cycle_critical".to_string())
        );
    }

    #[test]
    fn reject_reason_provider_skips_mode_eval_when_migration() {
        let out =
            reject_write_reason_with_mode_provider(true, || panic!("mode provider should not run"));
        assert_eq!(out, Some("ops.write.needs_migration".to_string()));
    }

    #[test]
    fn decode_label_ascii_and_hex() {
        let mut ascii = [0u8; 32];
        ascii[..9].copy_from_slice(b"ops.error");
        assert_eq!(
            decode_failure_label_view(ascii),
            Some("ops.error".to_string())
        );

        let mut non_ascii = [0u8; 32];
        non_ascii[..2].copy_from_slice(&[0xff, 0x10]);
        assert_eq!(
            decode_failure_label_view(non_ascii),
            Some("hex:ff10".to_string())
        );
    }
}
