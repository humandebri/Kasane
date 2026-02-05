//! どこで: wrapperログ設定の永続層 / 何を: LOG_FILTERの安定保存 / なぜ: env未設定時にも運用上書きを可能にするため

use crate::chain_data::codec::{encode_guarded, mark_decode_failure};
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;

pub const LOG_CONFIG_FILTER_MAX: usize = 96;
pub const LOG_CONFIG_SIZE_U32: u32 = 112;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogConfigV1 {
    pub has_filter: bool,
    pub filter: String,
}

impl LogConfigV1 {
    pub fn new() -> Self {
        Self {
            has_filter: false,
            filter: String::new(),
        }
    }

    pub fn filter(&self) -> Option<&str> {
        if self.has_filter {
            Some(self.filter.as_str())
        } else {
            None
        }
    }
}

impl Default for LogConfigV1 {
    fn default() -> Self {
        Self::new()
    }
}

impl Storable for LogConfigV1 {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let filter_bytes = self.filter.as_bytes();
        let len = u16::try_from(filter_bytes.len())
            .unwrap_or_else(|_| ic_cdk::trap("log_config: filter length overflow"));
        if usize::from(len) > LOG_CONFIG_FILTER_MAX {
            ic_cdk::trap("log_config: filter too long");
        }

        let mut out = [0u8; LOG_CONFIG_SIZE_U32 as usize];
        out[0] = u8::from(self.has_filter);
        out[2..4].copy_from_slice(&len.to_be_bytes());
        out[4..4 + usize::from(len)].copy_from_slice(filter_bytes);
        encode_guarded(b"log_config", out.to_vec(), LOG_CONFIG_SIZE_U32)
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != LOG_CONFIG_SIZE_U32 as usize {
            mark_decode_failure(b"log_config", false);
            return Self::new();
        }
        let has_filter = data[0] != 0;
        let mut len_buf = [0u8; 2];
        len_buf.copy_from_slice(&data[2..4]);
        let len = usize::from(u16::from_be_bytes(len_buf));
        if len > LOG_CONFIG_FILTER_MAX || 4 + len > data.len() {
            mark_decode_failure(b"log_config", false);
            return Self::new();
        }

        let filter = match std::str::from_utf8(&data[4..4 + len]) {
            Ok(v) => v.to_string(),
            Err(_) => {
                mark_decode_failure(b"log_config", false);
                return Self::new();
            }
        };

        Self { has_filter, filter }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: LOG_CONFIG_SIZE_U32,
        is_fixed_size: true,
    };
}
