import Std

namespace KasaneEvm

def deleteAccount (destroyed empty touched : Bool) : Bool :=
  destroyed || (empty && touched)

inductive CodeDecision where
  | skip | remove | insert
  deriving DecidableEq, Repr

def codeDecision (hasCode empty : Bool) : CodeDecision :=
  if !hasCode then .skip else if empty then .remove else .insert

theorem account_deleted_iff (destroyed empty touched : Bool) :
    deleteAccount destroyed empty touched = true ↔
      destroyed = true ∨ (empty = true ∧ touched = true) := by
  cases destroyed <;> cases empty <;> cases touched <;> decide

theorem absent_code_preserved (empty : Bool) : codeDecision false empty = .skip := by
  rfl

/-- A zero storage entry is absent, as in RevmStableDb::commit. -/
def storageValue (value : Nat) : Option Nat := if value = 0 then none else some value

def readStorage (value : Option Nat) : Nat := value.getD 0

theorem storage_roundtrip (value : Nat) : readStorage (storageValue value) = value := by
  by_cases h : value = 0 <;> simp [storageValue, readStorage, h]

/-- Logical map model. Byte encoding, deletion expansion, and stable-memory
operations must refine these writes; this is not a model of their implementation. -/
def applyWrites {K V : Type} [DecidableEq K]
    (state : K → Option V) (writes : List (K × Option V)) : K → Option V :=
  match writes with
  | [] => state
  | (key, value) :: rest =>
      applyWrites (fun k => if k = key then value else state k) rest

theorem untouched_key_preserved {K V : Type} [DecidableEq K]
    (state : K → Option V) (writes : List (K × Option V)) (key : K)
    (h : ∀ entry ∈ writes, entry.1 ≠ key) :
    applyWrites state writes key = state key := by
  induction writes generalizing state with
  | nil => rfl
  | cons entry rest ih =>
    have hn : key ≠ entry.1 := Ne.symm (h entry (by simp))
    have hr : ∀ e ∈ rest, e.1 ≠ key := by
      intro e he
      exact h e (by simp [he])
    simp only [applyWrites]
    rw [ih _ hr]
    simp [hn]

theorem last_write_wins {K V : Type} [DecidableEq K]
    (state : K → Option V) (earlier : List (K × Option V)) (key : K) (value : Option V) :
    applyWrites state (earlier ++ [(key, value)]) key = value := by
  induction earlier generalizing state with
  | nil => simp [applyWrites]
  | cons entry rest ih =>
    simp only [List.cons_append, applyWrites]
    exact ih _

end KasaneEvm
