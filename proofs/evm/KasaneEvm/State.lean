import Std

namespace KasaneEvm

inductive AccountDecision where
  | skip | delete | upsert
  deriving DecidableEq, Repr

def accountDecision (destroyed empty touched : Bool) : AccountDecision :=
  if !touched then .skip else if destroyed || empty then .delete else .upsert

def commitAccount (old : Option Nat) (new : Nat) : AccountDecision → Option Nat
  | .skip => old
  | .delete => none
  | .upsert => some new

structure StoredAccount where
  nonce : Nat
  balance : Nat
  codeEmpty : Bool
  deriving DecidableEq

def accountIsEmpty (account : StoredAccount) : Bool :=
  account.nonce == 0 && account.balance == 0 && account.codeEmpty

def readAccount (stored : Option StoredAccount) : Option StoredAccount :=
  match stored with
  | none => none
  | some account => if accountIsEmpty account then none else some account

theorem empty_account_read_as_absent (account : StoredAccount)
    (h : accountIsEmpty account = true) : readAccount (some account) = none := by
  simp [readAccount, h]

theorem nonempty_account_read_preserved (account : StoredAccount)
    (h : accountIsEmpty account = false) : readAccount (some account) = some account := by
  simp [readAccount, h]

inductive CodeDecision where
  | skip | remove | insert
  deriving DecidableEq, Repr

def codeDecision (hasCode empty : Bool) : CodeDecision :=
  if !hasCode then .skip else if empty then .remove else .insert

theorem account_deleted_iff (destroyed empty touched : Bool) :
    accountDecision destroyed empty touched = .delete ↔
      touched = true ∧ (destroyed = true ∨ empty = true) := by
  cases destroyed <;> cases empty <;> cases touched <;> decide

theorem untouched_account_preserved (destroyed empty : Bool)
    (old : Option Nat) (new : Nat) :
    commitAccount old new (accountDecision destroyed empty false) = old := by
  simp [accountDecision, commitAccount]

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
