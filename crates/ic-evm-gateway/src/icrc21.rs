//! どこで: gateway consent surface
//! 何を: Oisy signer 向け ICRC-21 consent message を生成
//! なぜ: submit_ic_tx を人間可読に承認させるため

use candid::{CandidType, Deserialize, Nat};
use evm_core::tx_decode::IcSyntheticTxInput;
use evm_core::wrap_precompile::WRAP_PRECOMPILE_ADDRESS;

use crate::{parse_submit_ic_tx_args, SubmitIcTxArgsDto};

const ICRC_10_URL: &str = "https://github.com/dfinity/ICRC/blob/main/ICRCs/ICRC-10/ICRC-10.md";
const ICRC_21_URL: &str = "https://github.com/dfinity/wg-identity-authentication/blob/main/topics/ICRC-21/icrc_21_consent_msg.md";
const ERC20_APPROVE_SELECTOR: [u8; 4] = [0x09, 0x5e, 0xa7, 0xb3];

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
pub struct StandardRecord {
    pub url: String,
    pub name: String,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
pub struct Icrc21ConsentMessageMetadata {
    pub utc_offset_minutes: Option<i16>,
    pub language: String,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
pub enum Icrc21DeviceSpec {
    GenericDisplay,
    LineDisplay(Icrc21LineDisplaySpec),
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
pub struct Icrc21LineDisplaySpec {
    pub characters_per_line: u16,
    pub lines_per_page: u16,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
pub struct Icrc21ConsentMessageSpec {
    pub metadata: Icrc21ConsentMessageMetadata,
    pub device_spec: Option<Icrc21DeviceSpec>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
pub struct Icrc21ConsentMessageRequest {
    pub arg: Vec<u8>,
    pub method: String,
    pub user_preferences: Icrc21ConsentMessageSpec,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
pub struct Icrc21ConsentInfo {
    pub metadata: Icrc21ConsentMessageMetadata,
    pub consent_message: Icrc21ConsentMessage,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
pub enum Icrc21ConsentMessage {
    GenericDisplayMessage(String),
    LineDisplayMessage(Icrc21LineDisplayMessage),
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
pub struct Icrc21LineDisplayMessage {
    pub pages: Vec<Icrc21LineDisplayPage>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
pub struct Icrc21LineDisplayPage {
    pub lines: Vec<String>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
pub struct Icrc21ErrorInfo {
    pub description: String,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
pub enum Icrc21Error {
    GenericError {
        description: String,
        error_code: Nat,
    },
    InsufficientPayment(Icrc21ErrorInfo),
    UnsupportedCanisterCall(Icrc21ErrorInfo),
    ConsentMessageUnavailable(Icrc21ErrorInfo),
}

pub type Icrc21ConsentMessageResponse = Result<Icrc21ConsentInfo, Icrc21Error>;

pub fn supported_standards() -> Vec<StandardRecord> {
    vec![
        StandardRecord {
            name: "ICRC-10".to_string(),
            url: ICRC_10_URL.to_string(),
        },
        StandardRecord {
            name: "ICRC-21".to_string(),
            url: ICRC_21_URL.to_string(),
        },
    ]
}

pub async fn consent_message(request: Icrc21ConsentMessageRequest) -> Icrc21ConsentMessageResponse {
    let metadata = request.user_preferences.metadata.clone();
    if request.method != "submit_ic_tx" {
        return Err(unsupported("icrc21.method_unsupported"));
    }
    let args = candid::decode_one::<SubmitIcTxArgsDto>(&request.arg)
        .map_err(|_| unsupported("icrc21.submit_ic_tx_decode_failed"))?;
    let tx = parse_submit_ic_tx_args(args)
        .map_err(|err| unavailable(&format!("icrc21.submit_ic_tx_parse_failed:{err:?}")))?;
    let markdown = describe_submit_ic_tx(&tx)?;
    Ok(Icrc21ConsentInfo {
        metadata,
        consent_message: Icrc21ConsentMessage::GenericDisplayMessage(markdown),
    })
}

fn describe_submit_ic_tx(tx: &IcSyntheticTxInput) -> Result<String, Icrc21Error> {
    if tx.to == Some(WRAP_PRECOMPILE_ADDRESS.into_array()) {
        return describe_precompile_unwrap(tx);
    }
    if is_erc20_approve(tx) {
        return describe_erc20_approve(tx);
    }
    Ok(format!(
        "# Approve Kasane transaction\n\n\
        - method: `submit_ic_tx`\n\
        - destination: `{}`\n\
        - value: `{}`\n\
        - data size: `{}` bytes\n\
        - nonce: `{}`\n\
        - gas limit: `{}`\n\
        - max fee per gas: `{}`\n\
        - max priority fee per gas: `{}`",
        format_address(tx.to),
        u256_to_decimal(&tx.value),
        tx.data.len(),
        tx.nonce,
        tx.gas_limit,
        tx.max_fee_per_gas,
        tx.max_priority_fee_per_gas,
    ))
}

fn describe_precompile_unwrap(tx: &IcSyntheticTxInput) -> Result<String, Icrc21Error> {
    let intent = decode_unwrap_payload(&tx.data)
        .ok_or_else(|| unsupported("icrc21.unwrap_payload_invalid"))?;
    Ok(format!(
        "# Approve Kasane unwrap\n\n\
        - method: `submit_ic_tx`\n\
        - target: `Kasane wrap precompile`\n\
        - asset principal: `{}`\n\
        - amount_e8s: `{}`\n\
        - recipient principal: `{}`\n\
        - nonce: `{}`\n\
        - gas limit: `{}`\n\
        - max fee per gas: `{}`\n\
        - max priority fee per gas: `{}`",
        intent.asset_principal,
        intent.amount_e8s,
        intent.recipient_principal,
        tx.nonce,
        tx.gas_limit,
        tx.max_fee_per_gas,
        tx.max_priority_fee_per_gas,
    ))
}

fn describe_erc20_approve(tx: &IcSyntheticTxInput) -> Result<String, Icrc21Error> {
    let approve = decode_erc20_approve(&tx.data)
        .ok_or_else(|| unsupported("icrc21.erc20_approve_invalid"))?;
    let spender = bytes_to_hex(&approve.spender);
    Ok(format!(
        "# Approve ERC-20 allowance transaction\n\n\
        - method: `submit_ic_tx`\n\
        - token address: `{}`\n\
        - spender: `0x{}`\n\
        - amount: `{}`\n\
        - nonce: `{}`\n\
        - gas limit: `{}`\n\
        - max fee per gas: `{}`\n\
        - max priority fee per gas: `{}`",
        format_address(tx.to),
        spender,
        approve.amount,
        tx.nonce,
        tx.gas_limit,
        tx.max_fee_per_gas,
        tx.max_priority_fee_per_gas,
    ))
}

struct UnwrapConsentView {
    asset_principal: String,
    amount_e8s: String,
    recipient_principal: String,
}

fn decode_unwrap_payload(data: &[u8]) -> Option<UnwrapConsentView> {
    const MAX_PRINCIPAL_LEN: usize = 29;
    if data.len() != 1 + (1 + MAX_PRINCIPAL_LEN) * 2 + 32 {
        return None;
    }
    if data[0] != 1 {
        return None;
    }
    let mut offset = 1usize;
    let asset = read_principal_field(data, &mut offset)?;
    let amount = read_array_32(data, &mut offset)?;
    let recipient = read_principal_field(data, &mut offset)?;
    if offset != data.len() {
        return None;
    }
    Some(UnwrapConsentView {
        asset_principal: candid::Principal::from_slice(&asset).to_text(),
        amount_e8s: u256_to_decimal(&amount),
        recipient_principal: candid::Principal::from_slice(&recipient).to_text(),
    })
}

struct Erc20ApproveView {
    spender: [u8; 20],
    amount: String,
}

fn is_erc20_approve(tx: &IcSyntheticTxInput) -> bool {
    tx.to.is_some() && tx.data.len() >= 68 && tx.data[..4] == ERC20_APPROVE_SELECTOR
}

fn decode_erc20_approve(data: &[u8]) -> Option<Erc20ApproveView> {
    if data.len() < 68 || data[..4] != ERC20_APPROVE_SELECTOR {
        return None;
    }
    let spender_slice = data.get(16..36)?;
    let amount_slice = data.get(36..68)?;
    let mut spender = [0u8; 20];
    spender.copy_from_slice(spender_slice);
    let mut amount = [0u8; 32];
    amount.copy_from_slice(amount_slice);
    Some(Erc20ApproveView {
        spender,
        amount: u256_to_decimal(&amount),
    })
}

fn read_principal_field(data: &[u8], offset: &mut usize) -> Option<Vec<u8>> {
    let len = usize::from(*data.get(*offset)?);
    if len == 0 || len > 29 {
        return None;
    }
    *offset += 1;
    let end = offset.checked_add(29)?;
    let field = data.get(*offset..end)?;
    *offset = end;
    Some(field.get(..len)?.to_vec())
}

fn read_array_32(data: &[u8], offset: &mut usize) -> Option<[u8; 32]> {
    let end = offset.checked_add(32)?;
    let bytes = data.get(*offset..end)?;
    *offset = end;
    let mut out = [0u8; 32];
    out.copy_from_slice(bytes);
    Some(out)
}

fn format_address(address: Option<[u8; 20]>) -> String {
    address
        .map(|value| format!("0x{}", bytes_to_hex(&value)))
        .unwrap_or_else(|| "null".to_string())
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn u256_to_decimal(bytes: &[u8; 32]) -> String {
    let value = num_bigint::BigUint::from_bytes_be(bytes);
    value.to_string()
}

fn unsupported(code: &str) -> Icrc21Error {
    Icrc21Error::UnsupportedCanisterCall(Icrc21ErrorInfo {
        description: code.to_string(),
    })
}

fn unavailable(code: &str) -> Icrc21Error {
    Icrc21Error::ConsentMessageUnavailable(Icrc21ErrorInfo {
        description: code.to_string(),
    })
}
