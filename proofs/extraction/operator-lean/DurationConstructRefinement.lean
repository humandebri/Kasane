import DurationConstructCorrespondence
import LedgerDurationSourceCorrespondence
open Aeneas Aeneas.Std
namespace DurationConstructRefinement
abbrev ReadNano := DurationConstruct.core.num.niche_types.Nanoseconds
abbrev ModelNano := LedgerDurationSource.core.num.niche_types.Nanoseconds
abbrev ReadDuration := DurationConstruct.core.time.Duration
abbrev ModelDuration := LedgerDurationSource.core.time.Duration

def toModelNano (n : ReadNano) : ModelNano :=
  ⟨n.val, DurationConstructCorrespondence.as_inner_range n⟩
def fromModelNano (n : ModelNano) : ReadNano :=
  ⟨n.value, Nat.zero_le _, by have h := n.valid; omega⟩
def toModel (d : ReadDuration) : ModelDuration := ⟨d.secs, toModelNano d.nanos⟩
def fromModel (d : ModelDuration) : ReadDuration := ⟨d.secs, fromModelNano d.nanos⟩

theorem nano_toModel_fromModel (n : ModelNano) : toModelNano (fromModelNano n) = n := by
  cases n
  rfl

theorem nano_fromModel_toModel (n : ReadNano) : fromModelNano (toModelNano n) = n := by
  cases n
  rfl

theorem toModel_fromModel (d : ModelDuration) : toModel (fromModel d) = d := by
  cases d with
  | mk secs nanos =>
    cases nanos
    rfl

theorem fromModel_toModel (d : ReadDuration) : fromModel (toModel d) = d := by
  cases d with
  | mk secs nanos =>
    cases nanos
    rfl

theorem as_inner_refines_model (n : ReadNano) :
    DurationConstruct.core.num.niche_types.Nanoseconds.as_inner n =
      LedgerDurationSource.core.num.niche_types.Nanoseconds.as_inner (toModelNano n) := by rfl

theorem as_nanos_refines_model (d : ReadDuration) :
    DurationConstruct.core.time.Duration.as_nanos d =
      LedgerDurationSource.core.time.Duration.as_nanos (toModel d) := by
  obtain ⟨u, hu, hv⟩ := DurationConstructCorrespondence.as_nanos_value d
  obtain ⟨v, hv', hvv⟩ := LedgerDurationSourceCorrespondence.as_nanos_value (toModel d)
  have he : u = v := by
    apply UScalar.eq_of_val_eq
    rw [hv, hvv]
    rfl
  rw [hu, hv', he]


theorem new_unchecked_refines_model (v : U32) (valid : v.val < 1000000000) :
    (do let n ← DurationConstruct.core.num.niche_types.Nanoseconds.new_unchecked v
        Result.ok (toModelNano n)) =
      LedgerDurationSource.core.num.niche_types.Nanoseconds.new_unchecked v := by
  obtain ⟨n, hn, hv⟩ := DurationConstructCorrespondence.new_unchecked_valid v valid
  have he : toModelNano n = ⟨v, valid⟩ := by
    cases n
    cases hv
    rfl
  simp [hn, he, LedgerDurationSource.core.num.niche_types.Nanoseconds.new_unchecked,
    dif_pos valid]

theorem model_value_injective (x y : ModelDuration)
    (h : LedgerDurationSourceCorrespondence.durationValue x =
      LedgerDurationSourceCorrespondence.durationValue y) : x = y := by
  have hx := x.nanos.valid
  have hy := y.nanos.valid
  unfold LedgerDurationSourceCorrespondence.durationValue at h
  have hs : x.secs = y.secs := UScalar.eq_of_val_eq (by omega)
  have hn : x.nanos.value = y.nanos.value := UScalar.eq_of_val_eq (by omega)
  cases x with
  | mk sx nx =>
    cases y with
    | mk sy ny =>
      cases nx
      cases ny
      cases hs
      cases hn
      rfl

theorem from_nanos_refines_model (n : U64) :
    (do let d ← DurationConstruct.core.time.Duration.from_nanos n
        Result.ok (toModel d)) =
      LedgerDurationSource.core.time.Duration.from_nanos n := by
  obtain ⟨d, hd, hv⟩ := DurationConstructCorrespondence.from_nanos_value n
  obtain ⟨m, hm, hmv⟩ := LedgerDurationSourceCorrespondence.from_nanos_value n
  have he : toModel d = m := by
    apply model_value_injective
    change DurationConstructCorrespondence.durationValue d =
      LedgerDurationSourceCorrespondence.durationValue m
    rw [hv, hmv]
  simp [hd, hm, he]
end DurationConstructRefinement
