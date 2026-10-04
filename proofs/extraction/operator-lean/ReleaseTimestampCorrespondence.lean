import ReleaseTimestampGenerated
open Aeneas Aeneas.Std
set_option maxHeartbeats 40000
namespace ReleaseTimestampSequence

theorem model_add_for (value : Duration → Nat) (t : Nat) (d : Duration) :
    execute (modelCallsFor value) t d releaseAdd [] =
      (if value d ≤ max64 then .returned (modelSaturation .add t (value d)) else .panic) := by
  change (match (if value d ≤ max64 then Except.ok (value d) else Except.error ()) with
    | .ok n => Outcome.returned (modelSaturation .add t n)
    | .error _ => Outcome.panic) = _
  by_cases h : value d ≤ max64
  · simp only [if_pos h]
  · simp only [if_neg h]

theorem model_add (t : Nat) (d : Duration) :
    execute modelCalls t d releaseAdd [] = expected .add t d :=
  model_add_for LedgerDuration.durationValue t d

theorem model_sub_for (value : Duration → Nat) (t : Nat) (d : Duration) :
    execute (modelCallsFor value) t d releaseSub [] =
      (if value d ≤ max64 then .returned (modelSaturation .sub t (value d)) else .panic) := by
  change (match (if value d ≤ max64 then Except.ok (value d) else Except.error ()) with
    | .ok n => Outcome.returned (modelSaturation .sub t n)
    | .error _ => Outcome.panic) = _
  by_cases h : value d ≤ max64
  · simp only [if_pos h]
  · simp only [if_neg h]

theorem model_sub (t : Nat) (d : Duration) :
    execute modelCalls t d releaseSub [] = expected .sub t d :=
  model_sub_for LedgerDuration.durationValue t d

theorem release_add_conditional (api : Calls) (contracts : CallContracts api)
    (t : U64) (d : Duration) :
    execute api t.val d releaseAdd [] = expected .add t.val d := by
  rw [calls_eq api contracts]
  exact model_add _ _

theorem release_sub_conditional (api : Calls) (contracts : CallContracts api)
    (t : U64) (d : Duration) :
    execute api t.val d releaseSub [] = expected .sub t.val d := by
  rw [calls_eq api contracts]
  exact model_sub _ _

theorem release_add_success (api : Calls) (contracts : CallContracts api)
    (t : U64) (d : Duration) (fits : LedgerDuration.durationValue d ≤ max64) :
    execute api t.val d releaseAdd [] =
      .returned (min max64 (t.val + LedgerDuration.durationValue d)) := by
  rw [release_add_conditional api contracts]
  simp [expected, fits, modelSaturation]

theorem release_sub_success (api : Calls) (contracts : CallContracts api)
    (t : U64) (d : Duration) (fits : LedgerDuration.durationValue d ≤ max64) :
    execute api t.val d releaseSub [] =
      .returned (t.val - LedgerDuration.durationValue d) := by
  rw [release_sub_conditional api contracts]
  simp [expected, fits, modelSaturation]

theorem release_add_overflow (api : Calls) (contracts : CallContracts api)
    (t : U64) (d : Duration) (over : max64 < LedgerDuration.durationValue d) :
    execute api t.val d releaseAdd [] = .panic := by
  rw [release_add_conditional api contracts]
  simp [expected, Nat.not_le.mpr over]

theorem release_sub_overflow (api : Calls) (contracts : CallContracts api)
    (t : U64) (d : Duration) (over : max64 < LedgerDuration.durationValue d) :
    execute api t.val d releaseSub [] = .panic := by
  rw [release_sub_conditional api contracts]
  simp [expected, Nat.not_le.mpr over]
end ReleaseTimestampSequence
