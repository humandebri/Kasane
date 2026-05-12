//! where: wrap-canister consent surface
//! what: Oisy signer 用の ICRC-21 consent message を生成
//! why: wrap/retry/recover を wallet signer で安全に承認させるため

use candid::{CandidType, Deserialize, Nat};

use crate::{
    init_state, nat_from_32_be, normalize_submit_wrap_args, principal_from_bytes,
    quote_wrap_request_inner, request_id_or_invalid_argument, request_overview_or_internal,
    with_state, RecoverFailedWrapArgs, RequestKind, RetryRequestArgs, SubmitWrapRequestArgs,
};

const ICRC_10_URL: &str = "https://github.com/dfinity/ICRC/blob/main/ICRCs/ICRC-10/ICRC-10.md";
const ICRC_21_URL: &str =
    "https://github.com/dfinity/wg-identity-authentication/blob/main/topics/ICRC-21/icrc_21_consent_msg.md";

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
    init_state();
    let metadata = request.user_preferences.metadata.clone();
    let markdown = match request.method.as_str() {
        "submit_wrap_request" => submit_wrap_request_consent(request.arg).await,
        "retry_request" => retry_request_consent(request.arg),
        "recover_failed_wrap" => recover_failed_wrap_consent(request.arg),
        _ => Err(unsupported("icrc21.method_unsupported")),
    }?;
    Ok(Icrc21ConsentInfo {
        metadata,
        consent_message: Icrc21ConsentMessage::GenericDisplayMessage(markdown),
    })
}

async fn submit_wrap_request_consent(arg: Vec<u8>) -> Result<String, Icrc21Error> {
    let args = candid::decode_one::<SubmitWrapRequestArgs>(&arg)
        .map_err(|_| unsupported("icrc21.submit_wrap_request_decode_failed"))?;
    let (
        asset_id,
        amount,
        evm_recipient,
        evm_nonce,
        gas_limit,
        max_fee_e8s,
        quoted_gas_price_wei,
        fee_ledger_canister,
    ) = normalize_submit_wrap_args(args).map_err(|code| invalid(&code))?;
    let quote = quote_wrap_request_inner(gas_limit)
        .await
        .map_err(|_| unavailable("icrc21.submit_wrap_request_quote_failed"))?;
    let asset_text = principal_from_bytes(&asset_id)
        .map_err(|_| invalid("icrc21.asset_principal_invalid"))?
        .to_text();
    let fee_ledger = quote.fee_ledger_canister.to_text();
    let amount_e8s = nat_from_32_be(&amount)
        .map_err(|_| invalid("icrc21.amount_invalid"))?
        .0
        .to_string();
    Ok(format!(
        "# Approve wrap request\n\n\
        - method: `submit_wrap_request`\n\
        - asset principal: `{asset_text}`\n\
        - amount_e8s: `{amount_e8s}`\n\
        - evm recipient: `0x{}`\n\
        - evm nonce: `{evm_nonce}`\n\
        - gas limit: `{gas_limit}`\n\
        - charged fee estimate (e8s): `{}`\n\
        - charged gas price (wei): `{}`\n\
        - max approved fee (e8s): `{max_fee_e8s}`\n\
        - max approved gas price (wei): `{quoted_gas_price_wei}`\n\
        - approved fee ledger canister: `{}`\n\
        - fee ledger canister: `{fee_ledger}`",
        bytes_to_hex(&evm_recipient),
        quote.charged_fee_e8s,
        quote.charged_gas_price_wei,
        fee_ledger_canister.to_text(),
    ))
}

fn retry_request_consent(arg: Vec<u8>) -> Result<String, Icrc21Error> {
    let args = candid::decode_one::<RetryRequestArgs>(&arg)
        .map_err(|_| unsupported("icrc21.retry_request_decode_failed"))?;
    let request_id = request_id_or_invalid_argument(&args.request_id)
        .map_err(|_| invalid("icrc21.retry_request_id_invalid"))?;
    let overview = request_overview_or_internal(request_id)
        .map_err(|_| unavailable("icrc21.retry_request_not_found"))?;
    if overview.kind != RequestKind::Unwrap {
        return Err(unsupported("icrc21.retry_request_wrong_kind"));
    }
    Ok(format!(
        "# Approve unwrap retry\n\n\
        - method: `retry_request`\n\
        - request id: `0x{}`\n\
        - request kind: `unwrap`\n\
        - current dispatch status: `{}`\n\
        - current execution status: `{:?}`",
        bytes_to_hex(&overview.request_id),
        format_optional_debug(overview.dispatch_status),
        overview.status,
    ))
}

fn recover_failed_wrap_consent(arg: Vec<u8>) -> Result<String, Icrc21Error> {
    let args = candid::decode_one::<RecoverFailedWrapArgs>(&arg)
        .map_err(|_| unsupported("icrc21.recover_failed_wrap_decode_failed"))?;
    let request_id = request_id_or_invalid_argument(&args.request_id)
        .map_err(|_| invalid("icrc21.recover_request_id_invalid"))?;
    let stored = with_state(|state| state.wrap_requests.get(&request_id));
    let Some(stored) = stored else {
        return Err(unavailable("icrc21.recover_request_not_found"));
    };
    if !stored.result.mint_failed_recoverable || stored.result.withdrawn {
        return Err(unavailable("icrc21.recover_request_unavailable"));
    }
    let asset = principal_from_bytes(stored.asset_id.as_slice())
        .map_err(|_| invalid("icrc21.recover_asset_invalid"))?
        .to_text();
    let amount = nat_from_32_be(stored.amount.as_slice())
        .map_err(|_| invalid("icrc21.recover_amount_invalid"))?
        .0
        .to_string();
    Ok(format!(
        "# Approve failed wrap recovery\n\n\
        - method: `recover_failed_wrap`\n\
        - request id: `0x{}`\n\
        - asset principal: `{asset}`\n\
        - refund amount_e8s: `{amount}`\n\
        - action: `withdraw failed wrap funds back to the caller ledger account`",
        bytes_to_hex(&request_id.0),
    ))
}

fn invalid(code: &str) -> Icrc21Error {
    Icrc21Error::GenericError {
        description: code.to_string(),
        error_code: Nat::from(1u8),
    }
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

fn format_optional_debug<T: std::fmt::Debug>(value: Option<T>) -> String {
    value
        .map(|item| format!("{item:?}"))
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
