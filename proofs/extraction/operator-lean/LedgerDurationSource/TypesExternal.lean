import Aeneas
open Aeneas Aeneas.Std
namespace LedgerDurationSource
-- Explicit constrained representation of opaque Rust Nanoseconds; transmute refinement unproved.
structure core.num.niche_types.Nanoseconds where
  value : U32
  valid : value.val < 1000000000
end LedgerDurationSource
