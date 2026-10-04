import LedgerDurationModel
open Aeneas Aeneas.Std
namespace ReleaseTimestampSequence
abbrev Duration := LedgerDuration.core.time.Duration
inductive Op where | add | sub deriving DecidableEq
inductive Value where | number (n : Nat) | converted (r : Except Unit Nat)
inductive Statement where
  | input (dst : Nat)
  | asNanos (dst : Nat)
  | narrow (dst src : Nat)
  | unwrap (dst src : Nat)
  | saturate (op : Op) (dst lhs rhs : Nat)
  | returnTimestamp (src : Nat)
inductive Outcome where | returned (n : Nat) | panic | invalid deriving DecidableEq
abbrev Store := List (Nat × Value)
def get : Store → Nat → Option Value
  | [], _ => none
  | (k, v) :: tail, i => if i = k then some v else get tail i
def put (s : Store) (i : Nat) (v : Value) : Store := (i, v) :: s
structure Calls where
  asNanos : Duration → Nat
  narrow : Nat → Except Unit Nat
  unwrap : Except Unit Nat → Except Unit Nat
  saturate : Op → Nat → Nat → Nat
-- Total asNanos/narrow/saturate and the value-or-panic observation are explicit abstraction choices.
def execute (api : Calls) (t : Nat) (d : Duration) : List Statement → Store → Outcome
  | [], _ => .invalid
  | .input dst :: tail, s => execute api t d tail (put s dst (.number t))
  | .asNanos dst :: tail, s => execute api t d tail (put s dst (.number (api.asNanos d)))
  | .narrow dst src :: tail, s => match get s src with
    | some (.number n) => execute api t d tail (put s dst (.converted (api.narrow n)))
    | _ => .invalid
  | .unwrap dst src :: tail, s => match get s src with
    | some (.converted r) => match api.unwrap r with
      | .ok n => execute api t d tail (put s dst (.number n))
      | .error _ => .panic
    | _ => .invalid
  | .saturate op dst lhs rhs :: tail, s => match get s lhs, get s rhs with
    | some (.number x), some (.number y) => execute api t d tail (put s dst (.number (api.saturate op x y)))
    | _, _ => .invalid
  | .returnTimestamp src :: _, s => match get s src with
    | some (.number n) => .returned n
    | _ => .invalid

def max64 : Nat := 18446744073709551615
def modelSaturation : Op → Nat → Nat → Nat
  | .add, x, y => min max64 (x + y)
  | .sub, x, y => x - y
def modelCallsFor (value : Duration → Nat) : Calls :=
  ⟨value, (fun n => if n ≤ max64 then .ok n else .error ()), id, modelSaturation⟩
def modelCalls : Calls := modelCallsFor LedgerDuration.durationValue
structure CallContracts (api : Calls) : Prop where
  asNanos : ∀ d, api.asNanos d = LedgerDuration.durationValue d
  narrow : ∀ n, api.narrow n = if n ≤ max64 then .ok n else .error ()
  unwrap : ∀ r, api.unwrap r = r
  saturate : ∀ op x y, api.saturate op x y = modelSaturation op x y

theorem calls_eq (api : Calls) (h : CallContracts api) : api = modelCalls := by
  cases api
  simp only [modelCalls, modelCallsFor, Calls.mk.injEq]
  exact ⟨funext h.asNanos, funext h.narrow, funext h.unwrap,
    funext (fun op => funext (fun x => funext (h.saturate op x)))⟩

def expected (op : Op) (t : Nat) (d : Duration) : Outcome :=
  if LedgerDuration.durationValue d ≤ max64 then
    .returned (modelSaturation op t (LedgerDuration.durationValue d)) else .panic
end ReleaseTimestampSequence
