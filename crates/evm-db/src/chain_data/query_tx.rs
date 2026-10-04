//! A single reserved query transaction; no provisional EVM mutations are persisted.
use super::icp_update_request::crc32_ieee;
use super::{codec::mark_decode_failure, constants::MAX_RETURN_DATA};
use candid::{CandidType, Deserialize};
use ic_stable_structures::{storable::Bound, Storable};
use std::borrow::Cow;

pub const QUERY_TX_TIMEOUT_SECONDS: u32 = 2;
pub const QUERY_TX_TIMEOUT_NANOS: u64 = QUERY_TX_TIMEOUT_SECONDS as u64 * 1_000_000_000;
pub const QUERY_TX_ARG_MAX: usize = 3_997;
const STATE_MAX_BYTES: u32 = (MAX_RETURN_DATA + QUERY_TX_ARG_MAX + 4096) as u32;

#[derive(Clone, Copy, Debug, Eq, PartialEq, CandidType, Deserialize)]
pub enum QueryTxPhase {
    Reserved,
    Waiting,
    Calling,
    Ready,
}

#[derive(Clone, Debug, Eq, PartialEq, CandidType, Deserialize)]
pub struct QueryTxSession {
    pub attempt_id: u64,
    pub tx_id: [u8; 32],
    pub phase: QueryTxPhase,
    pub block_number: u64,
    pub timestamp: u64,
    pub base_fee: u64,
    pub block_gas_limit: u64,
    pub snapshot: [u8; 32],
    pub update_active_count: u64,
    pub target: Vec<u8>,
    pub method: String,
    pub arg: Vec<u8>,
    pub started_at: u64,
    pub deadline: u64,
    pub reply: Option<Result<Vec<u8>, String>>,
}

#[derive(Clone, Debug, Eq, PartialEq, CandidType, Deserialize)]
pub struct QueryTxState {
    pub version: u8,
    pub next_attempt_id: u64,
    pub session: Option<QueryTxSession>,
}
impl Default for QueryTxState {
    fn default() -> Self {
        Self {
            version: 1,
            next_attempt_id: 1,
            session: None,
        }
    }
}
impl QueryTxState {
    pub fn valid(&self) -> bool {
        self.version == 1
            && self.next_attempt_id != 0
            && self.session.as_ref().is_none_or(|s| {
                s.attempt_id > 0
                    && s.attempt_id < self.next_attempt_id
                    && s.target.len() <= 29
                    && s.method.len() <= 64
                    && s.arg.len() <= QUERY_TX_ARG_MAX
                    && (s.phase == QueryTxPhase::Reserved
                        || (!s.target.is_empty() && !s.method.is_empty()))
                    && (s.phase == QueryTxPhase::Ready) == s.reply.is_some()
                    && s.reply.as_ref().is_none_or(|r| match r {
                        Ok(v) => v.len() <= MAX_RETURN_DATA,
                        Err(e) => e.len() <= 256,
                    })
            })
    }
}
impl Storable for QueryTxState {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        assert!(self.valid(), "invalid query tx state");
        let mut bytes = candid::encode_one(self).expect("encode query tx state");
        bytes.extend_from_slice(&crc32_ieee(&bytes).to_be_bytes());
        assert!(
            bytes.len() <= STATE_MAX_BYTES as usize,
            "query tx state too large"
        );
        Cow::Owned(bytes)
    }
    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }
    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let mut config = candid::de::DecoderConfig::new();
        config
            .set_decoding_quota(1_000_000)
            .set_skipping_quota(10_000);
        let payload = bytes.len().checked_sub(4).and_then(|end| {
            let checksum = u32::from_be_bytes(bytes[end..].try_into().ok()?);
            (crc32_ieee(&bytes[..end]) == checksum).then_some(&bytes[..end])
        });
        let decoded = (bytes.len() <= STATE_MAX_BYTES as usize)
            .then(|| {
                payload.and_then(|payload| {
                    candid::decode_one_with_config::<Self>(payload, &config).ok()
                })
            })
            .flatten();
        match decoded.filter(Self::valid) {
            Some(state) => state,
            None => {
                mark_decode_failure(b"query_tx_state", true);
                Self {
                    version: 0,
                    ..Self::default()
                }
            }
        }
    }
    const BOUND: Bound = Bound::Bounded {
        max_size: STATE_MAX_BYTES,
        is_fixed_size: false,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maximum_reply_roundtrips_within_stable_bound() {
        let state = QueryTxState {
            version: 1,
            next_attempt_id: 2,
            session: Some(QueryTxSession {
                attempt_id: 1,
                tx_id: [3; 32],
                phase: QueryTxPhase::Ready,
                block_number: 8,
                timestamp: 9,
                base_fee: 10,
                block_gas_limit: 30_000_000,
                snapshot: [4; 32],
                update_active_count: 7,
                target: vec![1; 29],
                method: "p".repeat(64),
                arg: vec![0; QUERY_TX_ARG_MAX],
                started_at: 1,
                deadline: QUERY_TX_TIMEOUT_NANOS + 1,
                reply: Some(Ok(vec![0; MAX_RETURN_DATA])),
            }),
        };
        assert_eq!(QueryTxState::from_bytes(state.to_bytes()), state);
    }
    #[test]
    fn invalid_phase_reply_pair_is_rejected() {
        let state = QueryTxState {
            version: 0,
            ..QueryTxState::default()
        };
        assert!(!state.valid());
        let state = QueryTxState {
            next_attempt_id: 0,
            ..QueryTxState::default()
        };
        assert!(!state.valid());
    }
}

#[cfg(test)]
mod corruption_tests {
    use super::*;
    #[test]
    fn checksum_corruption_stops_operations() {
        crate::stable_state::init_stable_state();
        crate::meta::clear_needs_migration();
        let mut bytes = QueryTxState::default().into_bytes();
        let end = bytes.len() - 1;
        bytes[end] ^= 1;
        assert!(!QueryTxState::from_bytes(Cow::Owned(bytes)).valid());
        assert!(crate::meta::needs_migration());
    }
}
