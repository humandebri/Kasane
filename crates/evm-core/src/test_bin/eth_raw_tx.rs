//! where: playground smoke helper; what: build a signed legacy raw tx; why: avoid python deps

use alloy_consensus::transaction::RlpEcdsaEncodableTx;
use alloy_consensus::{SignableTransaction, TxLegacy};
use alloy_primitives::B256;
use alloy_primitives::{Address, Bytes, Signature, TxKind, U256};
use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;
use evm_core::tx_decode::decode_eth_raw_tx;
use std::env;

fn parse_hex_bytes(value: &str) -> Result<Vec<u8>, String> {
    let trimmed = value.strip_prefix("0x").unwrap_or(value);
    hex::decode(trimmed).map_err(|_| "invalid hex".to_string())
}

fn parse_address(value: &str) -> Result<Address, String> {
    let bytes = parse_hex_bytes(value)?;
    if bytes.len() != 20 {
        return Err("address must be 20 bytes".to_string());
    }
    let mut buf = [0u8; 20];
    buf.copy_from_slice(&bytes);
    Ok(Address::from(buf))
}

fn parse_u64(value: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|_| format!("invalid u64: {value}"))
}

fn parse_u128(value: &str) -> Result<u128, String> {
    value
        .parse::<u128>()
        .map_err(|_| format!("invalid u128: {value}"))
}

fn parse_u256(value: &str) -> Result<U256, String> {
    let parsed = parse_u128(value)?;
    Ok(U256::from(parsed))
}

fn parse_csv_bytes(value: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    for item in value.split(';') {
        let part = item.trim();
        if part.is_empty() {
            continue;
        }
        let parsed = part
            .parse::<u16>()
            .map_err(|_| format!("invalid byte: {part}"))?;
        if parsed > u16::from(u8::MAX) {
            return Err(format!("byte out of range: {part}"));
        }
        out.push(parsed as u8);
    }
    if out.is_empty() {
        return Err("raw csv is empty".to_string());
    }
    Ok(out)
}

fn usage() -> String {
    "usage: eth_raw_tx --mode raw|sender|sender-hex|genkey|decode-csv --privkey HEX --to HEX --value WEI --gas-price WEI --gas-limit GAS --nonce NONCE --chain-id ID [--raw-csv \"1; 2; ...\"]"
        .to_string()
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut mode = "raw".to_string();
    let mut privkey = None;
    let mut to = None;
    let mut value = None;
    let mut gas_price = None;
    let mut gas_limit = None;
    let mut nonce = None;
    let mut chain_id = None;
    let mut raw_csv = None;

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--mode" => mode = args.next().ok_or_else(|| usage())?,
            "--privkey" => privkey = Some(args.next().ok_or_else(|| usage())?),
            "--to" => to = Some(args.next().ok_or_else(|| usage())?),
            "--value" => value = Some(args.next().ok_or_else(|| usage())?),
            "--gas-price" => gas_price = Some(args.next().ok_or_else(|| usage())?),
            "--gas-limit" => gas_limit = Some(args.next().ok_or_else(|| usage())?),
            "--nonce" => nonce = Some(args.next().ok_or_else(|| usage())?),
            "--chain-id" => chain_id = Some(args.next().ok_or_else(|| usage())?),
            "--raw-csv" => raw_csv = Some(args.next().ok_or_else(|| usage())?),
            _ => return Err(usage()),
        }
    }

    if mode == "genkey" {
        let signer = PrivateKeySigner::random();
        print_hex(signer.to_bytes());
        return Ok(());
    }

    let privkey = privkey.ok_or_else(|| usage())?;
    let signer: PrivateKeySigner = privkey.parse().map_err(|_| "invalid privkey")?;

    if mode == "sender" || mode == "sender-hex" {
        let sender = signer.address();
        if mode == "sender-hex" {
            for b in sender.as_slice() {
                print!("{b:02x}");
            }
            println!();
        } else {
            print_bytes(sender.as_slice());
        }
        return Ok(());
    }

    if mode == "decode-csv" {
        let csv = raw_csv.ok_or_else(|| usage())?;
        let raw = parse_csv_bytes(&csv)?;
        let decoded = decode_eth_raw_tx(&raw).map_err(|e| format!("decode failed: {e:?}"))?;
        let chain = decoded
            .chain_id
            .map(|v| v.to_string())
            .unwrap_or_else(|| "none".to_string());
        println!(
            "ok sender={} nonce={} chain_id={} tx_type={}",
            hex::encode(decoded.caller.as_slice()),
            decoded.nonce,
            chain,
            decoded.tx_type
        );
        return Ok(());
    }

    let to = parse_address(&to.ok_or_else(|| usage())?)?;
    let value = parse_u256(&value.ok_or_else(|| usage())?)?;
    let gas_price = parse_u128(&gas_price.ok_or_else(|| usage())?)?;
    let gas_limit = parse_u64(&gas_limit.ok_or_else(|| usage())?)?;
    let nonce = parse_u64(&nonce.ok_or_else(|| usage())?)?;
    let chain_id = parse_u64(&chain_id.ok_or_else(|| usage())?)?;

    let tx = TxLegacy {
        chain_id: Some(chain_id),
        nonce,
        gas_price,
        gas_limit,
        to: TxKind::Call(to),
        value,
        input: Bytes::new(),
    };

    let sig_hash = tx.signature_hash();
    let signature: Signature = signer
        .sign_hash_sync(&sig_hash)
        .map_err(|_| "signing failed")?;

    let mut out = Vec::with_capacity(tx.rlp_encoded_length_with_signature(&signature));
    tx.rlp_encode_signed(&signature, &mut out);
    decode_eth_raw_tx(&out).map_err(|e| format!("self-check decode failed: {e:?}"))?;
    print_bytes(&out);
    Ok(())
}

fn print_hex(bytes: B256) {
    println!("{}", hex::encode(bytes.as_slice()));
}

fn print_bytes(bytes: &[u8]) {
    let mut first = true;
    for b in bytes {
        if !first {
            print!("; ");
        }
        first = false;
        print!("{b}");
    }
    println!();
}
