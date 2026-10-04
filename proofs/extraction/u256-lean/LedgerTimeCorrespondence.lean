import LedgerTimeGenerated
import KasaneEvm.LedgerSaturation
set_option maxRecDepth 4096
open Aeneas Aeneas.Std
namespace LedgerTimeCorrespondence
abbrev TimeStamp := LedgerTime.timestamp.TimeStamp

theorem from_nanos_all_inputs (nanos : U64) :
    LedgerTime.timestamp.TimeStamp.from_nanos_since_unix_epoch nanos = .ok ⟨nanos⟩ := by rfl

theorem get_nanos_all_inputs (t : TimeStamp) :
    LedgerTime.timestamp.TimeStamp.as_nanos_since_unix_epoch t = .ok t.timestamp_nanos := by rfl

theorem timestamp_bound (t : TimeStamp) :
    t.timestamp_nanos.val ≤ KasaneEvm.LedgerSaturation.maxTime := by
  have h := t.timestamp_nanos.hBounds
  simp only [UScalarTy.numBits] at h
  unfold KasaneEvm.LedgerSaturation.maxTime
  omega

theorem roundtrip_all_inputs (nanos : U64) :
    (do let t ← LedgerTime.from_nanos nanos; LedgerTime.get_nanos t) = .ok nanos := by
  simp [LedgerTime.from_nanos, LedgerTime.get_nanos,
    LedgerTime.timestamp.TimeStamp.from_nanos_since_unix_epoch,
    LedgerTime.timestamp.TimeStamp.as_nanos_since_unix_epoch]

theorem new_valid (secs : U64) (nanos : U32)
    (range : nanos.val < 1000000000)
    (fits : secs.val * 1000000000 + nanos.val ≤ UScalar.max .U64) :
    ∃ t, LedgerTime.timestamp.TimeStamp.new secs nanos = .ok t ∧
      t.timestamp_nanos.val = secs.val * 1000000000 + nanos.val := by
  have hc : (UScalar.cast .U64 nanos).val = nanos.val := by simp
  have hcst : (1000000000#u64).val = 1000000000 := by rfl
  have hpbound : secs.val * (1000000000#u64).val ≤ UScalar.max .U64 := by
    rw [hcst]
    exact Nat.le_trans (Nat.le_add_right _ _) fits
  obtain ⟨product, hp, hpv⟩ := WP.spec_imp_exists
    (@UScalar.mul_spec .U64 secs 1000000000#u64 hpbound)
  rw [hcst] at hpv
  have habound : product.val + (UScalar.cast .U64 nanos).val ≤ UScalar.max .U64 := by
    rw [hpv, hc]
    exact fits
  obtain ⟨sum, ha, hav⟩ := WP.spec_imp_exists (UScalar.add_spec habound)
  refine ⟨⟨sum⟩, ?_, ?_⟩
  · simp [LedgerTime.timestamp.TimeStamp.new, UScalar.lt_equiv, range, hp, ha, lift]
  · change sum.val = _
    rw [hpv, hc] at hav
    exact hav

theorem new_invalid_nanos (secs : U64) (nanos : U32)
    (invalid : 1000000000 ≤ nanos.val) :
    LedgerTime.timestamp.TimeStamp.new secs nanos = .fail .assertionFailure := by
  have h : ¬ nanos.val < 1000000000 := by omega
  simp [LedgerTime.timestamp.TimeStamp.new, UScalar.lt_equiv, h]

theorem tryMk_overflow (n : Nat) (over : UScalar.max .U64 < n) :
    UScalar.tryMk .U64 n = .fail .integerOverflow := by
  have h : ¬ UScalar.check_bounds .U64 n := by
    rw [UScalar.check_bounds_eq_inBounds]
    simp only [UScalar.inBounds, UScalar.max, UScalarTy.numBits] at *
    omega
  unfold UScalar.tryMk UScalar.tryMkOpt
  rw [dif_neg h]
  rfl

theorem new_overflow (secs : U64) (nanos : U32)
    (range : nanos.val < 1000000000)
    (over : UScalar.max .U64 < secs.val * 1000000000 + nanos.val) :
    LedgerTime.timestamp.TimeStamp.new secs nanos = .fail .integerOverflow := by
  have hc : (UScalar.cast .U64 nanos).val = nanos.val := by simp
  have hcst : (1000000000#u64).val = 1000000000 := by rfl
  by_cases fits : secs.val * (1000000000#u64).val ≤ UScalar.max .U64
  · obtain ⟨product, hp, hpv⟩ := WP.spec_imp_exists
      (@UScalar.mul_spec .U64 secs 1000000000#u64 fits)
    rw [hcst] at hpv
    have ha : (product + UScalar.cast .U64 nanos : Result U64) = .fail .integerOverflow := by
      change UScalar.tryMk .U64 (product.val + (UScalar.cast .U64 nanos).val) = _
      apply tryMk_overflow
      rw [hpv, hc]
      exact over
    simp [LedgerTime.timestamp.TimeStamp.new, UScalar.lt_equiv, range, hp, ha, lift]
  · have hp : (secs * 1000000000#u64 : Result U64) = .fail .integerOverflow := by
      change UScalar.tryMk .U64 (secs.val * (1000000000#u64).val) = _
      apply tryMk_overflow
      omega
    simp [LedgerTime.timestamp.TimeStamp.new, UScalar.lt_equiv, range, hp]

theorem new_success_iff (secs : U64) (nanos : U32) :
    (∃ t, LedgerTime.timestamp.TimeStamp.new secs nanos = .ok t) ↔
      nanos.val < 1000000000 ∧ secs.val * 1000000000 + nanos.val ≤ UScalar.max .U64 := by
  constructor
  · intro success
    by_cases range : nanos.val < 1000000000
    · refine ⟨range, ?_⟩
      by_contra over
      have hf := new_overflow secs nanos range (by omega)
      obtain ⟨t, ht⟩ := success
      rw [hf] at ht
      simp at ht
    · have hf := new_invalid_nanos secs nanos (by omega)
      obtain ⟨t, ht⟩ := success
      rw [hf] at ht
      simp at ht
  · intro valid
    obtain ⟨t, ht, _⟩ := new_valid secs nanos valid.1 valid.2
    exact ⟨t, ht⟩

end LedgerTimeCorrespondence
