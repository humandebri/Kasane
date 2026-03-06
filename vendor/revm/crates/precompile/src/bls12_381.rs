//! BLS12-381 precompiles added in [`EIP-2537`](https://eips.ethereum.org/EIPS/eip-2537)
//! For more details check modules for each precompile.
#[cfg(feature = "bls12_381")]
use crate::Precompile;

#[allow(dead_code)]
pub(crate) mod arkworks;

cfg_if::cfg_if! {
    if #[cfg(feature = "blst")]{
        #[allow(dead_code)]
        pub(crate) mod blst;
        #[cfg(feature = "bls12_381")]
        pub(crate) use blst as crypto_backend;
    } else {
        #[cfg(feature = "bls12_381")]
        pub(crate) use arkworks as crypto_backend;
    }
}

// Re-export type aliases for use in submodules and shared KZG helpers.
use crate::bls12_381_const::{FP_LENGTH, SCALAR_LENGTH};
/// G1 point represented as two field elements (x, y coordinates)
pub type G1Point = ([u8; FP_LENGTH], [u8; FP_LENGTH]);
/// G2 point represented as four field elements (x0, x1, y0, y1 coordinates)
pub type G2Point = (
    [u8; FP_LENGTH],
    [u8; FP_LENGTH],
    [u8; FP_LENGTH],
    [u8; FP_LENGTH],
);
/// G1 point and scalar pair for MSM operations
pub type G1PointScalar = (G1Point, [u8; SCALAR_LENGTH]);
/// G2 point and scalar pair for MSM operations
pub type G2PointScalar = (G2Point, [u8; SCALAR_LENGTH]);
type PairingPair = (G1Point, G2Point);

#[cfg(feature = "bls12_381")]
pub mod g1_add;
#[cfg(feature = "bls12_381")]
pub mod g1_msm;
#[cfg(feature = "bls12_381")]
pub mod g2_add;
#[cfg(feature = "bls12_381")]
pub mod g2_msm;
#[cfg(feature = "bls12_381")]
pub mod map_fp2_to_g2;
#[cfg(feature = "bls12_381")]
pub mod map_fp_to_g1;
#[cfg(feature = "bls12_381")]
pub mod pairing;
mod pairing_common;
#[cfg(feature = "bls12_381")]
mod utils;

/// Returns the BLS12-381 precompiles with their addresses.
#[cfg(feature = "bls12_381")]
pub fn precompiles() -> impl Iterator<Item = Precompile> {
    [
        g1_add::PRECOMPILE,
        g1_msm::PRECOMPILE,
        g2_add::PRECOMPILE,
        g2_msm::PRECOMPILE,
        pairing::PRECOMPILE,
        map_fp_to_g1::PRECOMPILE,
        map_fp2_to_g2::PRECOMPILE,
    ]
    .into_iter()
}
