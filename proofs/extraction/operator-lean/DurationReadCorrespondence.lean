import DurationReadGenerated
open Aeneas Aeneas.Std
namespace DurationReadCorrespondence
open DurationRead

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

end DurationReadCorrespondence
