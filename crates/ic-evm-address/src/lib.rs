//! どこで: Principal->EVMアドレス導出共通層 / 何を: ic-pub-key準拠導出 / なぜ: 複数crateで同一ロジックを厳密共有するため

use alloy_primitives::keccak256;
use hex_literal::hex;
use ic_secp256k1::{DerivationIndex, DerivationPath, PublicKey};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SignerDerivationConfig {
    pub pubkey_sec1: [u8; 33],
    pub chaincode: [u8; 32],
    pub eth_domain_sep: [u8; 1],
}

pub const DEFAULT_SIGNER_DERIVATION_CONFIG: SignerDerivationConfig = SignerDerivationConfig {
    pubkey_sec1: hex!("0259761672ec7ee3bdc5eca95ba5f6a493d1133b86a76163b68af30c06fe3b75c0"),
    chaincode: hex!("f666a98c7f70fe281ca8142f14eb4d1e0934a439237da83869e2cfd924b270c0"),
    eth_domain_sep: [0x01],
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AddressDerivationError {
    InvalidMasterPublicKey,
    InvalidDerivedPublicKeyEncoding,
}

pub fn derive_evm_address_from_principal(
    principal_bytes: &[u8],
) -> Result<[u8; 20], AddressDerivationError> {
    derive_evm_address_from_principal_with_config(principal_bytes, DEFAULT_SIGNER_DERIVATION_CONFIG)
}

pub fn derive_evm_address_from_principal_with_config(
    principal_bytes: &[u8],
    config: SignerDerivationConfig,
) -> Result<[u8; 20], AddressDerivationError> {
    let path = DerivationPath::new(vec![
        DerivationIndex(config.eth_domain_sep.to_vec()),
        DerivationIndex(principal_bytes.to_vec()),
    ]);
    let master = PublicKey::deserialize_sec1(&config.pubkey_sec1)
        .map_err(|_| AddressDerivationError::InvalidMasterPublicKey)?;
    let (derived, _) = master.derive_subkey_with_chain_code(&path, &config.chaincode);
    let uncompressed = derived.serialize_sec1(false);
    derive_evm_address_from_uncompressed_sec1(uncompressed.as_slice())
}

fn derive_evm_address_from_uncompressed_sec1(
    uncompressed_sec1: &[u8],
) -> Result<[u8; 20], AddressDerivationError> {
    if uncompressed_sec1.len() != 65 || uncompressed_sec1[0] != 0x04 {
        return Err(AddressDerivationError::InvalidDerivedPublicKeyEncoding);
    }
    let hash = keccak256(&uncompressed_sec1[1..]).0;
    let mut out = [0u8; 20];
    out.copy_from_slice(&hash[12..32]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{
        derive_evm_address_from_principal, derive_evm_address_from_principal_with_config,
        derive_evm_address_from_uncompressed_sec1, AddressDerivationError, SignerDerivationConfig,
    };
    use candid::Principal;

    #[test]
    fn matches_ic_pub_key_reference_vector() {
        let principal =
            Principal::from_text("nggqm-p5ozz-i5hfv-bejmq-2gtow-4dtqw-vjatn-4b4yw-s5mzs-i46su-6ae")
                .expect("principal must parse");
        let address =
            derive_evm_address_from_principal(principal.as_slice()).expect("must derive address");
        assert_eq!(
            format!("0x{}", hex::encode(address)),
            "0xf53e047376e37eac56d48245b725c47410cf6f1e"
        );
    }

    #[test]
    fn deterministic_for_same_principal() {
        let principal = Principal::self_authenticating(b"deterministic-test");
        let a = derive_evm_address_from_principal(principal.as_slice()).expect("must derive");
        let b = derive_evm_address_from_principal(principal.as_slice()).expect("must derive");
        assert_eq!(a, b);
    }

    #[test]
    fn output_is_always_20_bytes() {
        let min_len = derive_evm_address_from_principal(&[]).expect("must derive");
        assert_eq!(min_len.len(), 20);

        let max_principal = vec![0u8; 29];
        let max_len =
            derive_evm_address_from_principal(max_principal.as_slice()).expect("must derive");
        assert_eq!(max_len.len(), 20);
    }

    #[test]
    fn with_config_returns_error_for_invalid_master_pubkey() {
        let cfg = SignerDerivationConfig {
            pubkey_sec1: [0u8; 33],
            chaincode: [0u8; 32],
            eth_domain_sep: [0x01],
        };
        let out = derive_evm_address_from_principal_with_config(b"principal", cfg);
        assert_eq!(out, Err(AddressDerivationError::InvalidMasterPublicKey));
    }

    #[test]
    fn uncompressed_pubkey_validation_rejects_invalid_shapes() {
        assert_eq!(
            derive_evm_address_from_uncompressed_sec1(&[]),
            Err(AddressDerivationError::InvalidDerivedPublicKeyEncoding)
        );
        assert_eq!(
            derive_evm_address_from_uncompressed_sec1(&[0u8; 64]),
            Err(AddressDerivationError::InvalidDerivedPublicKeyEncoding)
        );
        assert_eq!(
            derive_evm_address_from_uncompressed_sec1(&[0u8; 65]),
            Err(AddressDerivationError::InvalidDerivedPublicKeyEncoding)
        );
    }
}
