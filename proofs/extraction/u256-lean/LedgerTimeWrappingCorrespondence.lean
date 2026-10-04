import LedgerTimeWrappingGenerated
import LedgerTimeCorrespondence
open Aeneas Aeneas.Std
namespace LedgerTimeWrappingCorrespondence
abbrev TimeStamp := LedgerTimeWrapping.timestamp.TimeStamp

theorem new_valid_all_inputs (secs : U64) (nanos : U32)
    (range : nanos.val < 1000000000) :
    ∃ t, LedgerTimeWrapping.timestamp.TimeStamp.new secs nanos = .ok t ∧
      t.timestamp_nanos.val = (secs.val * 1000000000 + nanos.val) % (2^64) := by
  refine ⟨⟨U64.wrapping_add (U64.wrapping_mul secs 1000000000#u64)
    (UScalar.cast .U64 nanos)⟩, ?_, ?_⟩
  · simp [LedgerTimeWrapping.timestamp.TimeStamp.new, UScalar.lt_equiv, range, lift]
  · simp [U64.size, U64.numBits, Nat.add_mod]

theorem new_invalid_nanos (secs : U64) (nanos : U32)
    (invalid : 1000000000 ≤ nanos.val) :
    LedgerTimeWrapping.timestamp.TimeStamp.new secs nanos = .fail .assertionFailure := by
  have h : ¬ nanos.val < 1000000000 := by omega
  simp [LedgerTimeWrapping.timestamp.TimeStamp.new, UScalar.lt_equiv, h]

theorem new_success_iff (secs : U64) (nanos : U32) :
    (∃ t, LedgerTimeWrapping.timestamp.TimeStamp.new secs nanos = .ok t) ↔
      nanos.val < 1000000000 := by
  constructor
  · intro success
    by_contra invalid
    have hf := new_invalid_nanos secs nanos (by omega)
    obtain ⟨t, ht⟩ := success
    rw [hf] at ht
    simp at ht
  · intro range
    obtain ⟨t, ht, _⟩ := new_valid_all_inputs secs nanos range
    exact ⟨t, ht⟩

end LedgerTimeWrappingCorrespondence
