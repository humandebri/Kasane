//! どこで: gateway consent surface
//! 何を: Oisy signer 向け ICRC-21 consent message を生成
//! なぜ: EVM取引と資産の入出金を人間可読に承認させるため

use candid::{CandidType, Deserialize, Nat};
use evm_core::kasane_precompiles::WRAP_PRECOMPILE_ADDRESS;
use evm_core::tx_decode::IcSyntheticTxInput;

use crate::{
    normalize_submit_wrap_request, parse_submit_ic_tx_args, RecoverFailedWrapArgs,
    RetryRequestArgs, SubmitIcTxArgsDto, SubmitNativeDepositArgs, SubmitWrapRequestArgs,
};

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
    let markdown = match request.method.as_str() {
        "submit_ic_tx" => {
            let args = decode_arg::<SubmitIcTxArgsDto>(&request.arg, "submit_ic_tx")?;
            let tx = parse_submit_ic_tx_args(args)
                .map_err(|err| unavailable(&format!("icrc21.submit_ic_tx_parse_failed:{err:?}")))?;
            describe_submit_ic_tx(&tx)?
        }
        "submit_wrap_request" => describe_wrap(decode_arg(&request.arg, "submit_wrap_request")?)?,
        "submit_native_deposit" => {
            describe_native_deposit(decode_arg(&request.arg, "submit_native_deposit")?)?
        }
        "retry_request" | "retry_native_withdrawal" | "retry_native_deposit" => {
            let args: RetryRequestArgs = decode_arg(&request.arg, &request.method)?;
            describe_request_action(&request.method, args.request_id)?
        }
        "recover_failed_wrap" => {
            let args: RecoverFailedWrapArgs = decode_arg(&request.arg, "recover_failed_wrap")?;
            describe_request_action("recover_failed_wrap", args.request_id)?
        }
        _ => return Err(unsupported("icrc21.method_unsupported")),
    };
    Ok(Icrc21ConsentInfo {
        metadata,
        consent_message: Icrc21ConsentMessage::GenericDisplayMessage(markdown),
    })
}

fn decode_arg<T: CandidType + for<'de> Deserialize<'de>>(
    arg: &[u8],
    method: &str,
) -> Result<T, Icrc21Error> {
    candid::decode_one(arg).map_err(|_| unsupported(&format!("icrc21.{method}_decode_failed")))
}

fn describe_wrap(args: SubmitWrapRequestArgs) -> Result<String, Icrc21Error> {
    // Consent is available anonymously and describes signed arguments, not a
    // quote or request state that could change before the wallet submits it.
    normalize_submit_wrap_request(args.clone(), candid::Principal::anonymous())
        .map_err(|_| unsupported("icrc21.wrap_arguments_invalid"))?;
    Ok(format!(
        "# Approve Kasane wrap\n\n\
        Pull assets from your default ledger account and mint wrapped tokens to the EVM recipient.\n\n\
        - method: `submit_wrap_request`\n\
        - asset ledger: `{}`\n\
        - amount (asset ledger base units): `{}`\n\
        - EVM recipient: `0x{}`\n\
        - fee ledger: `{}`\n\
        - maximum Kasane fee (fee ledger base units): `{}`\n\
        - maximum gas price (wei): `{}`\n\
        - EVM nonce: `{}`\n\
        - gas limit: `{}`\n\n\
        The fee is charged separately from the asset amount, including when both use the same ledger. \
        Ledger transfer fees are additional and are not included in the maximum Kasane fee. \
        The call fails if the Kasane fee or gas price exceeds these limits. Minting is asynchronous; \
        submission does not guarantee completion. If minting fails after assets are pulled, \
        recovery must be requested separately.",
        args.asset_id,
        args.amount_e8s.0,
        bytes_to_hex(&args.evm_recipient),
        args.fee_ledger_canister,
        args.max_fee_e8s.0,
        args.quoted_gas_price_wei.0,
        args.evm_nonce,
        args.gas_limit,
    ))
}

fn describe_native_deposit(args: SubmitNativeDepositArgs) -> Result<String, Icrc21Error> {
    if args.deposit_id.len() != 32
        || crate::validate_evm_address(&args.evm_recipient, "arg.evm_recipient_invalid").is_err()
        || args.amount_e8s == 0u8
        || crate::native_deposit_amount_wei_bytes(&args.amount_e8s).is_err()
        || crate::nat_to_u128(&args.max_fee_e8s).is_none()
    {
        return Err(unsupported("icrc21.native_deposit_arguments_invalid"));
    }
    Ok(format!(
        "# Approve Kasane native deposit\n\n\
        Pull funds from your default account on the native ledger configured at execution \
        and credit the EVM recipient's native balance.\n\n\
        - method: `submit_native_deposit`\n\
        - deposit ID: `0x{}`\n\
        - amount (native ledger e8s): `{}`\n\
        - EVM recipient: `0x{}`\n\
        - fee ledger: `{}`\n\
        - maximum Kasane fee (fee ledger base units): `{}`\n\n\
        The fee is charged separately from the deposit amount. Ledger transfer fees are \
        additional and are not included in the maximum Kasane fee. Credit can fail after funds \
        are pulled; retry_native_deposit retries credit without pulling the deposit again.",
        bytes_to_hex(&args.deposit_id),
        args.amount_e8s.0,
        bytes_to_hex(&args.evm_recipient),
        args.fee_ledger_canister,
        args.max_fee_e8s.0,
    ))
}

fn describe_request_action(method: &str, request_id: Vec<u8>) -> Result<String, Icrc21Error> {
    let id = crate::tx_id_from_bytes(request_id)
        .ok_or_else(|| unsupported("icrc21.request_id_invalid"))?;
    let action = match method {
        "recover_failed_wrap" => {
            "Request a refund of the recorded asset amount to the original \
            depositor's default ledger account after a failed wrap. The fee already charged is \
            not refunded. This does not mint wrapped tokens or debit your wallet again."
        }
        "retry_native_deposit" => {
            "Retry credit of the recorded deposit to its original EVM \
            recipient. This does not pull the deposit from your wallet again."
        }
        _ => {
            "Retry the recorded ledger payout to its original recipient. This does not change \
            its asset, amount, or recipient, or burn tokens again."
        }
    };
    Ok(format!(
        "# Approve Kasane request recovery\n\n\
        - method: `{method}`\n\
        - request ID: `0x{}`\n\n\
        {action}\n\n\
        Execution depends on the stored request. A missing, completed, or ineligible request \
        can be rejected. Approval does not guarantee a payout or successful credit.",
        bytes_to_hex(&id.0),
    ))
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
        - amount (asset ledger base units): `{}`\n\
        - recipient principal: `{}`\n\
        - native value (wei): `{}`\n\
        - nonce: `{}`\n\
        - gas limit: `{}`\n\
        - max fee per gas: `{}`\n\
        - max priority fee per gas: `{}`",
        intent.asset_principal,
        intent.amount_e8s,
        intent.recipient_principal,
        u256_to_decimal(&tx.value),
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
        - native value (wei): `{}`\n\
        - nonce: `{}`\n\
        - gas limit: `{}`\n\
        - max fee per gas: `{}`\n\
        - max priority fee per gas: `{}`\n\n\
        The calldata requests ERC-20 approve; actual effects depend on the destination contract. \
        Native value is sent in addition to transaction gas fees.",
        format_address(tx.to),
        spender,
        approve.amount,
        u256_to_decimal(&tx.value),
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
    tx.to.is_some() && decode_erc20_approve(&tx.data).is_some()
}

fn decode_erc20_approve(data: &[u8]) -> Option<Erc20ApproveView> {
    if data.len() != 68
        || data[..4] != ERC20_APPROVE_SELECTOR
        || data[4..16].iter().any(|&byte| byte != 0)
    {
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
    if field[len..].iter().any(|&byte| byte != 0) {
        return None;
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use candid::{encode_one, Principal};

    fn consent<T: CandidType>(method: &str, args: &T) -> Icrc21ConsentMessageResponse {
        consent_bytes(method, encode_one(args).unwrap())
    }

    fn consent_bytes(method: &str, arg: Vec<u8>) -> Icrc21ConsentMessageResponse {
        crate::tests::run_ready_future(consent_message(Icrc21ConsentMessageRequest {
            method: method.into(),
            arg,
            user_preferences: Icrc21ConsentMessageSpec {
                metadata: Icrc21ConsentMessageMetadata {
                    language: "en".into(),
                    utc_offset_minutes: None,
                },
                device_spec: None,
            },
        }))
    }

    fn markdown(response: Icrc21ConsentMessageResponse) -> String {
        match response.unwrap().consent_message {
            Icrc21ConsentMessage::GenericDisplayMessage(text) => text,
            _ => panic!("expected generic display"),
        }
    }

    fn wrap_args() -> SubmitWrapRequestArgs {
        SubmitWrapRequestArgs {
            asset_id: Principal::self_authenticating(b"asset"),
            amount_e8s: Nat::from(123456789u64),
            evm_recipient: vec![0x12; 20],
            evm_nonce: 7,
            gas_limit: 3000000,
            max_fee_e8s: Nat::from(987u64),
            quoted_gas_price_wei: Nat::from(654u64),
            fee_ledger_canister: Principal::self_authenticating(b"fee"),
        }
    }

    #[test]
    fn wrap_displays_signed_ledgers_amount_recipient_and_caps() {
        let args = wrap_args();
        let text = markdown(consent("submit_wrap_request", &args));
        for expected in [
            args.asset_id.to_text(),
            args.fee_ledger_canister.to_text(),
            "123456789".into(),
            "987".into(),
            "654".into(),
            format!("0x{}", "12".repeat(20)),
            "EVM nonce: `7`".into(),
            "gas limit: `3000000`".into(),
            "asset ledger base units".into(),
            "charged separately".into(),
            "submission does not guarantee completion".into(),
        ] {
            assert!(text.contains(&expected), "missing {expected}");
        }
    }

    #[test]
    fn wrap_rejects_invalid_inputs_and_wrong_candid() {
        let mut args = wrap_args();
        args.evm_recipient.pop();
        assert!(consent("submit_wrap_request", &args).is_err());
        args = wrap_args();
        args.amount_e8s = Nat::from(0u8);
        assert!(consent("submit_wrap_request", &args).is_err());
        args = wrap_args();
        args.max_fee_e8s = Nat(num_bigint::BigUint::from(1u8) << 128);
        assert!(consent("submit_wrap_request", &args).is_err());
        assert!(consent("submit_wrap_request", &42u64).is_err());
    }

    #[test]
    fn native_deposit_displays_inputs_and_rejects_invalid_inputs() {
        let mut args = SubmitNativeDepositArgs {
            deposit_id: vec![0xab; 32],
            amount_e8s: Nat::from(123u64),
            evm_recipient: vec![0x34; 20],
            max_fee_e8s: Nat::from(456u64),
            fee_ledger_canister: Principal::self_authenticating(b"fee"),
        };
        let text = markdown(consent("submit_native_deposit", &args));
        for expected in [
            "ab".repeat(32),
            "34".repeat(20),
            "123".into(),
            "456".into(),
            args.fee_ledger_canister.to_text(),
            "configured at execution".into(),
            "without pulling the deposit again".into(),
        ] {
            assert!(text.contains(&expected), "missing {expected}");
        }
        args.deposit_id.pop();
        assert!(consent("submit_native_deposit", &args).is_err());
        args.deposit_id.push(0xab);
        args.amount_e8s = Nat::from(0u8);
        assert!(consent("submit_native_deposit", &args).is_err());
    }

    #[test]
    fn recovery_consent_is_available_without_request_state() {
        for method in [
            "retry_request",
            "retry_native_withdrawal",
            "retry_native_deposit",
            "recover_failed_wrap",
        ] {
            let args = RetryRequestArgs {
                request_id: vec![0xcd; 32],
            };
            let text = markdown(consent(method, &args));
            assert!(text.contains(method));
            assert!(text.contains(&"cd".repeat(32)));
            assert!(text.contains("ineligible request"));
            if method == "recover_failed_wrap" {
                assert!(text.contains("original depositor"));
                assert!(text.contains("not refunded"));
            }
            assert!(consent(
                method,
                &RetryRequestArgs {
                    request_id: vec![0; 31]
                }
            )
            .is_err());
        }
        assert!(consent("unknown_method", &42u64).is_err());
    }

    #[test]
    fn unwrap_rejects_nonzero_principal_padding() {
        let mut payload = vec![0; 93];
        payload[0] = 1;
        payload[1] = 1;
        payload[2] = 1;
        payload[62] = 1;
        payload[63] = 1;
        payload[64] = 1;
        assert!(decode_unwrap_payload(&payload).is_some());
        for index in (3..31).chain(65..93) {
            let mut malformed = payload.clone();
            malformed[index] = 1;
            assert!(
                decode_unwrap_payload(&malformed).is_none(),
                "padding byte {index}"
            );
        }
    }

    #[test]
    fn approve_with_extra_calldata_or_noncanonical_address_is_generic() {
        let mut tx = IcSyntheticTxInput {
            to: Some([0x22; 20]),
            value: [0; 32],
            gas_limit: 90000,
            nonce: 1,
            max_fee_per_gas: 3,
            max_priority_fee_per_gas: 2,
            data: vec![0; 68],
        };
        tx.data[..4].copy_from_slice(&ERC20_APPROVE_SELECTOR);
        assert!(is_erc20_approve(&tx));
        tx.data.push(1);
        assert!(!is_erc20_approve(&tx));
        tx.data.pop();
        for index in 4..16 {
            tx.data[index] = 1;
            assert!(!is_erc20_approve(&tx), "address padding byte {index}");
            tx.data[index] = 0;
        }
    }

    #[test]
    fn consent_rejects_every_truncated_wrap_argument() {
        let bytes = encode_one(wrap_args()).unwrap();
        for length in 0..bytes.len() {
            assert!(
                consent_bytes("submit_wrap_request", bytes[..length].to_vec()).is_err(),
                "truncated length {length}"
            );
        }
    }

    #[test]
    fn wrap_amount_and_fee_boundaries_match_execution_normalizer() {
        let mut args = wrap_args();
        for bit in [0, 1, 127, 128, 255, 256, 257] {
            for below in [false, true] {
                args.amount_e8s = Nat((num_bigint::BigUint::from(1u8) << bit) - u8::from(below));
                let accepted =
                    normalize_submit_wrap_request(args.clone(), Principal::anonymous()).is_ok();
                assert_eq!(
                    consent("submit_wrap_request", &args).is_ok(),
                    accepted,
                    "amount bit {bit}, below {below}"
                );
            }
        }
        args = wrap_args();
        for value in [
            Nat::from(u128::MAX),
            Nat(num_bigint::BigUint::from(1u8) << 128),
        ] {
            args.max_fee_e8s = value.clone();
            assert_eq!(
                consent("submit_wrap_request", &args).is_ok(),
                crate::nat_to_u128(&value).is_some()
            );
            args.max_fee_e8s = Nat::from(1u8);
            args.quoted_gas_price_wei = value.clone();
            assert_eq!(
                consent("submit_wrap_request", &args).is_ok(),
                crate::nat_to_u128(&value).is_some()
            );
            args.quoted_gas_price_wei = Nat::from(1u8);
        }
    }

    #[test]
    fn native_conversion_overflow_and_address_lengths_are_rejected() {
        let mut args = SubmitNativeDepositArgs {
            deposit_id: vec![0xab; 32],
            amount_e8s: Nat::from(u128::MAX / crate::WEI_PER_E8S),
            evm_recipient: vec![0x12; 20],
            max_fee_e8s: Nat::from(u128::MAX),
            fee_ledger_canister: Principal::self_authenticating(b"fee"),
        };
        assert!(consent("submit_native_deposit", &args).is_ok());
        args.amount_e8s = Nat::from(u128::MAX / crate::WEI_PER_E8S + 1);
        assert!(consent("submit_native_deposit", &args).is_err());
        args.amount_e8s = Nat::from(1u8);
        for length in [0, 1, 19, 21, 32] {
            args.evm_recipient = vec![0x12; length];
            assert!(consent("submit_native_deposit", &args).is_err());
        }
    }
}
