//! どこで: Principal->EVMアドレス導出共通層 / 何を: ic-pub-key準拠導出 / なぜ: 複数crateで同一ロジックを厳密共有するため

use alloy_primitives::keccak256;
use ic_pub_key::{derive_ecdsa_key, EcdsaCurve, EcdsaKeyId, EcdsaPublicKeyArgs};
use ic_secp256k1::PublicKey;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AddressDerivationError {
    InvalidSignerCanisterId,
    DerivationFailed,
    InvalidDerivedPublicKeyEncoding,
}

const ETH_DOMAIN_SEPARATOR: &[u8] = &[0x01];
const CHAIN_FUSION_SIGNER_CANISTER_ID: &str = "grghe-syaaa-aaaar-qabyq-cai";
const KEY_ID_KEY_1: &str = "key_1";
static SIGNER_CANISTER_ID_BYTES: OnceLock<Result<Vec<u8>, AddressDerivationError>> = OnceLock::new();

pub fn derive_evm_address_from_principal(
    principal_bytes: &[u8],
) -> Result<[u8; 20], AddressDerivationError> {
    let signer_canister_id_bytes = signer_canister_id_bytes()?;
    let signer_canister_id = ic_pub_key::CanisterId::from_slice(signer_canister_id_bytes);
    let out = derive_ecdsa_key(&EcdsaPublicKeyArgs {
        canister_id: Some(signer_canister_id),
        derivation_path: vec![ETH_DOMAIN_SEPARATOR.to_vec(), principal_bytes.to_vec()],
        key_id: EcdsaKeyId {
            curve: EcdsaCurve::Secp256k1,
            name: KEY_ID_KEY_1.to_string(),
        },
    })
    .map_err(|_| AddressDerivationError::DerivationFailed)?;
    let derived = PublicKey::deserialize_sec1(&out.public_key)
        .map_err(|_| AddressDerivationError::InvalidDerivedPublicKeyEncoding)?;
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

fn signer_canister_id_bytes() -> Result<&'static [u8], AddressDerivationError> {
    match SIGNER_CANISTER_ID_BYTES.get_or_init(|| {
        let principal = ic_pub_key::CanisterId::from_text(CHAIN_FUSION_SIGNER_CANISTER_ID)
            .map_err(|_| AddressDerivationError::InvalidSignerCanisterId)?;
        Ok(principal.as_slice().to_vec())
    }) {
        Ok(bytes) => Ok(bytes.as_slice()),
        Err(err) => Err(*err),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        derive_evm_address_from_principal, derive_evm_address_from_uncompressed_sec1,
        AddressDerivationError,
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
