import LedgerDurationExtract.TypesExternal
open Aeneas Aeneas.Std
namespace LedgerDuration

def durationValue (d : core.time.Duration) : Nat :=
  d.secs.val * 1000000000 + d.nanos.val

theorem durationValue_bound (d : core.time.Duration) : durationValue d < 2^128 := by
  have hs := d.secs.hBounds
  have hn := d.nanos_range
  simp only [UScalarTy.numBits] at hs
  unfold durationValue
  omega

def core.time.Duration.as_nanos (d : core.time.Duration) : Result U128 :=
  .ok (U128.ofNatCore (durationValue d) (durationValue_bound d))

def core.time.Duration.from_nanos (n : U64) : Result core.time.Duration :=
  .ok {
    secs := U64.ofNatCore (n.val / 1000000000) (by
      have h := n.hBounds
      simp only [UScalarTy.numBits] at h ⊢
      omega)
    nanos := U32.ofNatCore (n.val % 1000000000) (by
      have h := Nat.mod_lt n.val (by decide : 0 < 1000000000)
      simp only [UScalarTy.numBits]
      omega)
    nanos_range := by
      simp only [U32.ofNatCore_val_eq]
      exact Nat.mod_lt _ (by decide) }
end LedgerDuration
