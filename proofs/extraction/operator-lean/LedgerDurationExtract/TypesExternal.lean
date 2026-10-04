import Aeneas
open Aeneas Aeneas.Std
-- Explicit interface model; refinement to Rust's constrained Nanoseconds is unproved.
namespace LedgerDuration
structure core.time.Duration where
  secs : U64
  nanos : U32
  nanos_range : nanos.val < 1000000000
end LedgerDuration
