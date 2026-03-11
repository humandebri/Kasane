use std::{borrow::Cow, fmt};

use crate::{Precompile, PrecompileSpecId};

/// Precompile with address and function.
/// Unique precompile identifier.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PrecompileId {
    /// Elliptic curve digital signature algorithm (ECDSA) public key recovery function.
    EcRec,
    /// SHA2-256 hash function.
    Sha256,
    /// RIPEMD-160 hash function.
    Ripemd160,
    /// Identity precompile.
    Identity,
    /// Arbitrary-precision exponentiation under modulo.
    ModExp,
    /// Point addition (ADD) on the elliptic curve 'alt_bn128'.
    Bn254Add,
    /// Scalar multiplication (MUL) on the elliptic curve 'alt_bn128'.
    Bn254Mul,
    /// Bilinear function on groups on the elliptic curve 'alt_bn128'.
    Bn254Pairing,
    /// Compression function F used in the BLAKE2 cryptographic hashing algorithm.
    Blake2F,
    /// Verify p(z) = y given commitment that corresponds to the polynomial p(x) and a KZG proof. Also verify that the provided commitment matches the provided versioned_hash.
    KzgPointEvaluation,
    /// Point addition in G1 (curve over base prime field).
    Bls12G1Add,
    /// Multi-scalar-multiplication (MSM) in G1 (curve over base prime field).
    Bls12G1Msm,
    /// Point addition in G2 (curve over quadratic extension of the base prime field).
    Bls12G2Add,
    /// Multi-scalar-multiplication (MSM) in G2 (curve over quadratic extension of the base prime field).
    Bls12G2Msm,
    /// Pairing operations between a set of pairs of (G1, G2) points.
    Bls12Pairing,
    /// Base field element mapping into the G1 point.
    Bls12MapFpToGp1,
    /// Extension field element mapping into the G2 point.
    Bls12MapFp2ToGp2,
    /// ECDSA signature verification over the secp256r1 elliptic curve (also known as P-256 or prime256v1).
    P256Verify,
    /// Custom precompile identifier.
    Custom(Cow<'static, str>),
}

impl PrecompileId {
    /// Create new custom precompile ID.
    pub fn custom<I>(id: I) -> Self
    where
        I: Into<Cow<'static, str>>,
    {
        Self::Custom(id.into())
    }

    /// Returns the name of the precompile as defined in EIP-7910.
    pub fn name(&self) -> &str {
        match self {
            Self::EcRec => "ECREC",
            Self::Sha256 => "SHA256",
            Self::Ripemd160 => "RIPEMD160",
            Self::Identity => "ID",
            Self::ModExp => "MODEXP",
            Self::Bn254Add => "BN254_ADD",
            Self::Bn254Mul => "BN254_MUL",
            Self::Bn254Pairing => "BN254_PAIRING",
            Self::Blake2F => "BLAKE2F",
            Self::KzgPointEvaluation => "KZG_POINT_EVALUATION",
            Self::Bls12G1Add => "BLS12_G1ADD",
            Self::Bls12G1Msm => "BLS12_G1MSM",
            Self::Bls12G2Add => "BLS12_G2ADD",
            Self::Bls12G2Msm => "BLS12_G2MSM",
            Self::Bls12Pairing => "BLS12_PAIRING_CHECK",
            Self::Bls12MapFpToGp1 => "BLS12_MAP_FP_TO_G1",
            Self::Bls12MapFp2ToGp2 => "BLS12_MAP_FP2_TO_G2",
            Self::P256Verify => "P256VERIFY",
            Self::Custom(a) => a.as_ref(),
        }
    }

    /// Returns the precompile function for the given spec.
    ///
    /// If case of [`PrecompileId::Custom`] it will return [`None`].
    ///
    /// For case where precompile was still not introduced in the spec,
    /// it will return [`Some`] with fork closest to activation.
    pub fn precompile(&self, spec: PrecompileSpecId) -> Option<Precompile> {
        use PrecompileSpecId::*;

        let precompile = match self {
            Self::EcRec => crate::secp256k1::ECRECOVER,
            Self::Sha256 => crate::hash::SHA256,
            Self::Ripemd160 => crate::hash::RIPEMD160,
            Self::Identity => crate::identity::FUN,
            Self::ModExp => {
                // ModExp changes gas calculation based on spec
                if spec < BERLIN {
                    crate::modexp::BYZANTIUM
                } else if spec < OSAKA {
                    crate::modexp::BERLIN
                } else {
                    crate::modexp::OSAKA
                }
            }
            Self::Bn254Add => {
                // BN254 add - gas cost changes in Istanbul
                if spec < ISTANBUL {
                    crate::bn254::add::BYZANTIUM
                } else {
                    crate::bn254::add::ISTANBUL
                }
            }
            Self::Bn254Mul => {
                // BN254 mul - gas cost changes in Istanbul
                if spec < ISTANBUL {
                    crate::bn254::mul::BYZANTIUM
                } else {
                    crate::bn254::mul::ISTANBUL
                }
            }
            Self::Bn254Pairing => {
                // BN254 pairing - gas cost changes in Istanbul
                if spec < ISTANBUL {
                    crate::bn254::pair::BYZANTIUM
                } else {
                    crate::bn254::pair::ISTANBUL
                }
            }
            Self::Blake2F => crate::blake2::FUN,
            Self::KzgPointEvaluation => {
                #[cfg(feature = "kzg_precompile")]
                {
                    crate::kzg_point_evaluation::POINT_EVALUATION
                }
                #[cfg(not(feature = "kzg_precompile"))]
                return None;
            }
            Self::Bls12G1Add => {
                #[cfg(feature = "bls12_381")]
                {
                    crate::bls12_381::g1_add::PRECOMPILE
                }
                #[cfg(not(feature = "bls12_381"))]
                return None;
            }
            Self::Bls12G1Msm => {
                #[cfg(feature = "bls12_381")]
                {
                    crate::bls12_381::g1_msm::PRECOMPILE
                }
                #[cfg(not(feature = "bls12_381"))]
                return None;
            }
            Self::Bls12G2Add => {
                #[cfg(feature = "bls12_381")]
                {
                    crate::bls12_381::g2_add::PRECOMPILE
                }
                #[cfg(not(feature = "bls12_381"))]
                return None;
            }
            Self::Bls12G2Msm => {
                #[cfg(feature = "bls12_381")]
                {
                    crate::bls12_381::g2_msm::PRECOMPILE
                }
                #[cfg(not(feature = "bls12_381"))]
                return None;
            }
            Self::Bls12Pairing => {
                #[cfg(feature = "bls12_381")]
                {
                    crate::bls12_381::pairing::PRECOMPILE
                }
                #[cfg(not(feature = "bls12_381"))]
                return None;
            }
            Self::Bls12MapFpToGp1 => {
                #[cfg(feature = "bls12_381")]
                {
                    crate::bls12_381::map_fp_to_g1::PRECOMPILE
                }
                #[cfg(not(feature = "bls12_381"))]
                return None;
            }
            Self::Bls12MapFp2ToGp2 => {
                #[cfg(feature = "bls12_381")]
                {
                    crate::bls12_381::map_fp2_to_g2::PRECOMPILE
                }
                #[cfg(not(feature = "bls12_381"))]
                return None;
            }
            Self::P256Verify => {
                // P256 verify - gas cost changes in Osaka
                #[cfg(feature = "secp256r1")]
                if spec < OSAKA {
                    crate::secp256r1::P256VERIFY
                } else {
                    crate::secp256r1::P256VERIFY_OSAKA
                }
                #[cfg(not(feature = "secp256r1"))]
                return None;
            }
            Self::Custom(_) => return None,
        };

        Some(precompile)
    }
}

impl fmt::Display for PrecompileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::{PrecompileId, PrecompileSpecId};

    #[test]
    fn test_bls12_precompile_ids_are_feature_gated() {
        let ids = [
            PrecompileId::Bls12G1Add,
            PrecompileId::Bls12G1Msm,
            PrecompileId::Bls12G2Add,
            PrecompileId::Bls12G2Msm,
            PrecompileId::Bls12Pairing,
            PrecompileId::Bls12MapFpToGp1,
            PrecompileId::Bls12MapFp2ToGp2,
        ];

        for id in ids {
            #[cfg(feature = "bls12_381")]
            assert!(id.precompile(PrecompileSpecId::PRAGUE).is_some());

            #[cfg(not(feature = "bls12_381"))]
            assert!(id.precompile(PrecompileSpecId::PRAGUE).is_none());
        }
    }

    #[test]
    fn test_kzg_precompile_id_is_feature_gated() {
        #[cfg(feature = "kzg_precompile")]
        assert!(PrecompileId::KzgPointEvaluation
            .precompile(PrecompileSpecId::CANCUN)
            .is_some());

        #[cfg(not(feature = "kzg_precompile"))]
        assert!(PrecompileId::KzgPointEvaluation
            .precompile(PrecompileSpecId::CANCUN)
            .is_none());
    }
}
