import DurationConstructGenerated
open Aeneas Aeneas.Std
namespace DurationConstructCorrespondence
open DurationConstruct

theorem as_inner_all_inputs (n : core.num.niche_types.Nanoseconds) :
    core.num.niche_types.Nanoseconds.as_inner n = .ok n.val := by rfl

theorem as_inner_range (n : core.num.niche_types.Nanoseconds) : n.val.val < 1000000000 := by
  have h := n.property.2
  omega

theorem as_secs_all_inputs (d : core.time.Duration) :
    core.time.Duration.as_secs d = .ok d.secs := by rfl

def durationValue (d : core.time.Duration) : Nat :=
  d.secs.val * 1000000000 + d.nanos.val.val

theorem durationValue_bound (d : core.time.Duration) : durationValue d < 2^128 := by
  have hs := d.secs.hBounds
  have hn := d.nanos.property.2
  simp only [UScalarTy.numBits] at hs
  unfold durationValue
  omega

theorem as_nanos_value (d : core.time.Duration) :
    ∃ u, core.time.Duration.as_nanos d = .ok u ∧ u.val = durationValue d := by
  have hconst : UScalar.cast .U128 1000000000#u32 = 1000000000#u128 := by rfl
  have hsecs : (UScalar.cast .U128 d.secs).val = d.secs.val := by simp
  have hnanos : (UScalar.cast .U128 d.nanos.val).val = d.nanos.val.val := by simp
  have hproduct : (UScalar.cast .U128 d.secs).val * (1000000000#u128).val ≤
      UScalar.max .U128 := by
    rw [hsecs]
    have h := durationValue_bound d
    unfold durationValue at h
    simp only [UScalar.max, UScalarTy.numBits]
    change d.secs.val * 1000000000 ≤ 2^128 - 1
    omega
  obtain ⟨product, hp, hpv⟩ := WP.spec_imp_exists
    (UScalar.mul_spec hproduct)
  have hsum : product.val + (UScalar.cast .U128 d.nanos.val).val ≤
      UScalar.max .U128 := by
    rw [hpv, hsecs, hnanos]
    have h := durationValue_bound d
    unfold durationValue at h
    simp only [UScalar.max, UScalarTy.numBits]
    change d.secs.val * 1000000000 + d.nanos.val.val ≤ 2^128 - 1
    omega
  obtain ⟨sum, ha, hav⟩ := WP.spec_imp_exists (UScalar.add_spec hsum)
  refine ⟨sum, ?_, ?_⟩
  · simp [core.time.Duration.as_nanos, core.time.NANOS_PER_SEC,
      core.num.niche_types.Nanoseconds.as_inner, lift, hconst, hp, ha]
  · rw [hav, hpv, hsecs, hnanos]
    rfl



theorem new_unchecked_valid (v : U32) (valid : v.val < 1000000000) :
    ∃ n, core.num.niche_types.Nanoseconds.new_unchecked v = .ok n ∧ n.val = v := by
  have upper : v.val ≤ 999999999 := by omega
  refine ⟨⟨v, Nat.zero_le _, upper⟩, ?_, rfl⟩
  simp only [core.num.niche_types.Nanoseconds.new_unchecked, dif_pos upper]

-- This is the candidate's undefined-input representation, not a Rust runtime result.
theorem new_unchecked_undefined_model (v : U32) (invalid : 1000000000 ≤ v.val) :
    core.num.niche_types.Nanoseconds.new_unchecked v = .fail .undef := by
  have upper : ¬ v.val ≤ 999999999 := by omega
  simp only [core.num.niche_types.Nanoseconds.new_unchecked, dif_neg upper]

theorem new_unchecked_read_roundtrip (v : U32) (valid : v.val < 1000000000) :
    (do let n ← core.num.niche_types.Nanoseconds.new_unchecked v
        core.num.niche_types.Nanoseconds.as_inner n) = .ok v := by
  obtain ⟨n, hn, hv⟩ := new_unchecked_valid v valid
  simp [hn, core.num.niche_types.Nanoseconds.as_inner, hv]

theorem read_construct_roundtrip (n : core.num.niche_types.Nanoseconds) :
    core.num.niche_types.Nanoseconds.new_unchecked n.val = .ok n := by
  simp only [core.num.niche_types.Nanoseconds.new_unchecked, dif_pos n.property.2]

theorem from_nanos_value (n : U64) :
    ∃ d, core.time.Duration.from_nanos n = .ok d ∧ durationValue d = n.val := by
  have hconst : UScalar.cast .U64 1000000000#u32 = 1000000000#u64 := by rfl
  obtain ⟨secs, hs, hsv⟩ := UScalar.div_spec n
    (y := 1000000000#u64) (by decide)
  obtain ⟨rem, hr, hrv⟩ := WP.spec_imp_exists
    (UScalar.rem_spec n (y := 1000000000#u64) (by decide))
  have hrem : rem.val < 1000000000 := by
    rw [hrv]
    exact Nat.mod_lt _ (by decide)
  have hcast : (UScalar.cast .U32 rem).val = rem.val := by
    apply UScalar.cast_val_mod_pow_of_inBounds_eq
    simp only [UScalarTy.numBits]
    omega
  have hvalid : (UScalar.cast .U32 rem).val < 1000000000 := by omega
  have upper : (UScalar.cast .U32 rem).val ≤ 999999999 := by omega
  have hnew : core.num.niche_types.Nanoseconds.new_unchecked (UScalar.cast .U32 rem) =
      .ok ⟨UScalar.cast .U32 rem, Nat.zero_le _, upper⟩ := by
    simp only [core.num.niche_types.Nanoseconds.new_unchecked, dif_pos upper]
  refine ⟨⟨secs, ⟨UScalar.cast .U32 rem, Nat.zero_le _, upper⟩⟩, ?_, ?_⟩
  · simp [core.time.Duration.from_nanos,
      core.time.Duration.from_nanos.NANOS_PER_SEC, core.time.NANOS_PER_SEC,
      hconst, hs, hr, lift, hnew]
  · change secs.val * 1000000000 + (UScalar.cast .U32 rem).val = n.val
    rw [hcast, hsv, hrv]
    change (n.val / 1000000000) * 1000000000 + n.val % 1000000000 = n.val
    omega


theorem from_nanos_cast_valid (n : U64) :
    ∃ (rem : U64), (n % 1000000000#u64 : Result U64) = .ok rem ∧
      (UScalar.cast .U32 rem).val < 1000000000 := by
  obtain ⟨rem, hr, hrv⟩ := WP.spec_imp_exists
    (UScalar.rem_spec n (y := 1000000000#u64) (by decide))
  have hrem : rem.val < 1000000000 := by
    rw [hrv]
    exact Nat.mod_lt _ (by decide)
  have hcast : (UScalar.cast .U32 rem).val = rem.val := by
    apply UScalar.cast_val_mod_pow_of_inBounds_eq
    simp only [UScalarTy.numBits]
    omega
  exact ⟨rem, hr, by omega⟩

theorem from_nanos_as_nanos_roundtrip (n : U64) :
    (do let d ← core.time.Duration.from_nanos n
        core.time.Duration.as_nanos d) = .ok (UScalar.cast .U128 n) := by
  obtain ⟨d, hd, hv⟩ := from_nanos_value n
  obtain ⟨u, hu, huv⟩ := as_nanos_value d
  have he : u = UScalar.cast .U128 n := by
    apply UScalar.eq_of_val_eq
    rw [huv, hv]
    simp
  simp [hd, hu, he]
end DurationConstructCorrespondence
