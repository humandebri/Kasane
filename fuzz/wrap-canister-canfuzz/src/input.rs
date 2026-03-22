//! where: wrap-canister canfuzz input parser
//! what: convert raw fuzzer bytes into candid arguments with a bias toward valid states
//! why: purely random candid blobs spend most executions in decode failures and do not reach logic branches

use candid::{CandidType, Deserialize, Nat, Principal};
use num_bigint::BigUint;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DispatchInput {
    pub(crate) request_id: Vec<u8>,
    pub(crate) asset_id: Principal,
    pub(crate) amount_e8s: Nat,
    pub(crate) recipient: Principal,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub(crate) struct DispatchUnwrapRequestArgs {
    pub(crate) request_id: Vec<u8>,
    pub(crate) asset_id: Principal,
    pub(crate) amount_e8s: Nat,
    pub(crate) recipient: Principal,
}

impl DispatchInput {
    pub(crate) fn from_bytes(bytes: &[u8]) -> Self {
        let mut cursor = Cursor::new(bytes);
        let mode = cursor.byte();
        let request_id_len = request_id_len(mode, cursor.byte());
        let request_id = cursor.bytes(request_id_len, 0xAB);
        let asset_id = principal_from_cursor(&mut cursor, mode & 0x02 != 0, b"asset");
        let recipient = principal_from_cursor(&mut cursor, mode & 0x04 != 0, b"recipient");
        let amount_len = usize::from(cursor.byte() % 17);
        let amount_bytes = cursor.bytes(amount_len, 0);

        Self {
            request_id,
            asset_id,
            amount_e8s: Nat::from(BigUint::from_bytes_be(&amount_bytes)),
            recipient,
        }
    }

    pub(crate) fn to_args(&self) -> DispatchUnwrapRequestArgs {
        DispatchUnwrapRequestArgs {
            request_id: self.request_id.clone(),
            asset_id: self.asset_id,
            amount_e8s: self.amount_e8s.clone(),
            recipient: self.recipient,
        }
    }
}

fn request_id_len(mode: u8, next: u8) -> usize {
    if mode & 0x01 == 0 {
        return 32;
    }
    usize::from(next % 40)
}

fn principal_from_cursor(cursor: &mut Cursor<'_>, allow_anonymous: bool, salt: &[u8]) -> Principal {
    if allow_anonymous && cursor.byte() & 0x01 == 0 {
        return Principal::anonymous();
    }

    let body_len = usize::from((cursor.byte() % 16) + 1);
    let mut body = salt.to_vec();
    body.extend(cursor.bytes(body_len, salt[0]));
    Principal::self_authenticating(&body)
}

#[derive(Clone, Debug)]
struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn byte(&mut self) -> u8 {
        if self.offset >= self.bytes.len() {
            return 0;
        }
        let byte = self.bytes[self.offset];
        self.offset += 1;
        byte
    }

    fn bytes(&mut self, len: usize, filler: u8) -> Vec<u8> {
        let mut out = Vec::with_capacity(len);
        for _ in 0..len {
            out.push(if self.offset < self.bytes.len() {
                let byte = self.bytes[self.offset];
                self.offset += 1;
                byte
            } else {
                filler
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::DispatchInput;
    use candid::Principal;

    #[test]
    fn parser_defaults_to_valid_request_id_length() {
        let input = DispatchInput::from_bytes(&[]);
        assert_eq!(input.request_id.len(), 32);
        assert_ne!(input.asset_id, Principal::anonymous());
        assert_ne!(input.recipient, Principal::anonymous());
    }

    #[test]
    fn parser_can_generate_edge_case_lengths() {
        let input = DispatchInput::from_bytes(&[1, 7]);
        assert_eq!(input.request_id.len(), 7);
    }

    #[test]
    fn parser_can_emit_anonymous_principals_for_auth_paths() {
        let input = DispatchInput::from_bytes(&[0x06, 0, 0, 0]);
        assert_eq!(input.asset_id, Principal::anonymous());
        assert_eq!(input.recipient, Principal::anonymous());
    }
}
