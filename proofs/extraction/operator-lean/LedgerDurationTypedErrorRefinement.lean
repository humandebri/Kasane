import LedgerDurationTypedErrorCorrespondence
import LedgerDurationModel
open Aeneas Aeneas.Std
namespace LedgerDurationTypedErrorRefinement
abbrev SourceDuration := LedgerDurationTypedError.core.time.Duration
abbrev ModelDuration := LedgerDuration.core.time.Duration

def toModel (d : SourceDuration) : ModelDuration :=
  ⟨d.secs, d.nanos.val, by have h := d.nanos.property.2; omega⟩

def fromModel (d : ModelDuration) : SourceDuration :=
  ⟨d.secs, ⟨d.nanos, Nat.zero_le _, by have h := d.nanos_range; omega⟩⟩

theorem toModel_fromModel (d : ModelDuration) : toModel (fromModel d) = d := by
  cases d
  rfl

theorem fromModel_toModel (d : SourceDuration) : fromModel (toModel d) = d := by
  cases d
  rfl

theorem model_value_injective (x y : ModelDuration)
    (h : LedgerDuration.durationValue x = LedgerDuration.durationValue y) : x = y := by
  have hx := x.nanos_range
  have hy := y.nanos_range
  unfold LedgerDuration.durationValue at h
  have hs : x.secs = y.secs := UScalar.eq_of_val_eq (by omega)
  have hn : x.nanos = y.nanos := UScalar.eq_of_val_eq (by omega)
  cases x
  cases y
  cases hs
  cases hn
  rfl

theorem as_nanos_refines_model (d : SourceDuration) :
    LedgerDurationTypedError.core.time.Duration.as_nanos d =
      LedgerDuration.core.time.Duration.as_nanos (toModel d) := by
  obtain ⟨u, hu, hv⟩ := LedgerDurationTypedErrorCorrespondence.as_nanos_value d
  have hm : LedgerDuration.core.time.Duration.as_nanos (toModel d) =
      .ok (U128.ofNatCore (LedgerDuration.durationValue (toModel d))
        (LedgerDuration.durationValue_bound (toModel d))) := rfl
  have hmv := U128.ofNatCore_val_eq (LedgerDuration.durationValue_bound (toModel d))
  have he : u = U128.ofNatCore (LedgerDuration.durationValue (toModel d))
      (LedgerDuration.durationValue_bound (toModel d)) := by
    apply UScalar.eq_of_val_eq
    rw [hv, hmv]
    rfl
  rw [hu, hm, he]

theorem from_nanos_refines_model (n : U64) :
    (do let d ← LedgerDurationTypedError.core.time.Duration.from_nanos n; Result.ok (toModel d)) =
      LedgerDuration.core.time.Duration.from_nanos n := by
  obtain ⟨d, hd, hv⟩ := LedgerDurationTypedErrorCorrespondence.from_nanos_value n
  have hm : ∃ m, LedgerDuration.core.time.Duration.from_nanos n = .ok m ∧
      LedgerDuration.durationValue m = n.val := by
    refine ⟨_, rfl, ?_⟩
    simp only [LedgerDuration.durationValue, U64.ofNatCore_val_eq, U32.ofNatCore_val_eq]
    omega
  obtain ⟨m, hm, hmv⟩ := hm
  have he : toModel d = m := by
    apply model_value_injective
    change LedgerDurationTypedErrorCorrespondence.durationValue d = LedgerDuration.durationValue m
    rw [hv, hmv]
  simp [hd, hm, he]
end LedgerDurationTypedErrorRefinement
