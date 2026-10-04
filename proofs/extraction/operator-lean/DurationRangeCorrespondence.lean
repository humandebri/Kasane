import DurationRangeGenerated
import LedgerDurationSource.TypesExternal
open Aeneas Aeneas.Std
namespace DurationRangeCorrespondence
abbrev Nanoseconds := DurationRangeFields.core.num.niche_types.Nanoseconds
abbrev ModelNanoseconds := LedgerDurationSource.core.num.niche_types.Nanoseconds
abbrev Duration := DurationRangeFields.core.time.Duration

theorem range_preserved (n : Nanoseconds) : n.val.val < 1000000000 := by
  have h := n.property.2
  omega

def toModel (n : Nanoseconds) : ModelNanoseconds := ⟨n.val, range_preserved n⟩
def fromModel (n : ModelNanoseconds) : Nanoseconds :=
  ⟨n.value, Nat.zero_le _, by have h := n.valid; omega⟩

theorem toModel_fromModel (n : ModelNanoseconds) : toModel (fromModel n) = n := by
  cases n
  rfl

theorem fromModel_toModel (n : Nanoseconds) : fromModel (toModel n) = n := by
  cases n
  rfl

theorem model_payload_preserved (n : Nanoseconds) : (toModel n).value = n.val := by rfl

theorem as_secs_all_inputs (d : Duration) :
    DurationRangeFields.core.time.Duration.as_secs d = .ok d.secs := by rfl
end DurationRangeCorrespondence
