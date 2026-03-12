use candid::{decode_one, Principal};
use evm_core::hash;
use ic_evm_gateway::InitArgs;

fn decode_hex(input: &str) -> Vec<u8> {
    let bytes = input.as_bytes();
    assert_eq!(bytes.len() % 2, 0, "hex length must be even");
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let hi = (pair[0] as char).to_digit(16).expect("valid hex") as u8;
        let lo = (pair[1] as char).to_digit(16).expect("valid hex") as u8;
        out.push((hi << 4) | lo);
    }
    out
}

fn canbench_init_args_hex() -> &'static str {
    include_str!("../../../canbench.yml")
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("hex:")
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .expect("canbench init_args.hex")
}

#[test]
fn canbench_yaml_init_args_match_gateway_type() {
    let raw = decode_hex(canbench_init_args_hex());
    let decoded: Option<InitArgs> = decode_one(&raw).expect("decode canbench init args");
    let decoded = decoded.expect("init args must be present");

    assert_eq!(decoded.genesis_balances.len(), 1);
    assert_eq!(decoded.genesis_balances[0].address.len(), 20);
    assert_ne!(decoded.genesis_balances[0].amount, 0);
    assert_ne!(decoded.wrap_canister_id, Principal::anonymous());
    assert_eq!(decoded.wrap_factory_address.len(), 20);
    assert_eq!(
        decoded.genesis_balances[0].address,
        hash::derive_evm_address_from_principal(decoded.wrap_canister_id.as_slice())
            .expect("derive evm address")
            .to_vec()
    );
}
