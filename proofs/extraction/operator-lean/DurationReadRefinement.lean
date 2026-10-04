import DurationReadCorrespondence
import LedgerDurationSourceCorrespondence
open Aeneas Aeneas.Std
namespace DurationReadRefinement
abbrev ReadNano := DurationRead.core.num.niche_types.Nanoseconds
abbrev ModelNano := LedgerDurationSource.core.num.niche_types.Nanoseconds
abbrev ReadDuration := DurationRead.core.time.Duration
abbrev ModelDuration := LedgerDurationSource.core.time.Duration

def toModelNano (n : ReadNano) : ModelNano :=
  ⟨n.val, DurationReadCorrespondence.as_inner_range n⟩
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
    DurationRead.core.num.niche_types.Nanoseconds.as_inner n =
      LedgerDurationSource.core.num.niche_types.Nanoseconds.as_inner (toModelNano n) := by rfl

theorem as_nanos_refines_model (d : ReadDuration) :
    DurationRead.core.time.Duration.as_nanos d =
      LedgerDurationSource.core.time.Duration.as_nanos (toModel d) := by
  obtain ⟨u, hu, hv⟩ := DurationReadCorrespondence.as_nanos_value d
  obtain ⟨v, hv', hvv⟩ := LedgerDurationSourceCorrespondence.as_nanos_value (toModel d)
  have he : u = v := by
    apply UScalar.eq_of_val_eq
    rw [hv, hvv]
    rfl
  rw [hu, hv', he]
end DurationReadRefinement
