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

theorem read_account_idempotent (stored : Option StoredAccount) :
    readAccount (readAccount stored) = readAccount stored := by
  cases stored with
  | none => rfl
  | some account =>
    by_cases h : accountIsEmpty account = true
    · simp [readAccount, h]
    · have hf : accountIsEmpty account = false := Bool.eq_false_iff.mpr h
      simp [readAccount, hf]

theorem empty_account_iff (account : StoredAccount) :
    accountIsEmpty account = true ↔
      account.nonce = 0 ∧ account.balance = 0 ∧ account.codeEmpty = true := by
  simp [accountIsEmpty, and_assoc]

theorem account_upserted_iff (destroyed empty touched : Bool) :
    accountDecision destroyed empty touched = .upsert ↔
      touched = true ∧ destroyed = false ∧ empty = false := by
  cases destroyed <;> cases empty <;> cases touched <;> decide

theorem storage_zero_iff (value : Nat) : storageValue value = none ↔ value = 0 := by
  by_cases h : value = 0 <;> simp [storageValue, h]

theorem apply_writes_append {K V : Type} [DecidableEq K]
    (state : K → Option V) (first second : List (K × Option V)) :
    applyWrites state (first ++ second) = applyWrites (applyWrites state first) second := by
  induction first generalizing state with
  | nil => rfl
  | cons entry rest ih =>
    simp only [List.cons_append, applyWrites]
    exact ih _

/-- Two input maps that agree at a key still agree there after the same writes. -/
theorem apply_writes_locality {K V : Type} [DecidableEq K]
    (left right : K → Option V) (writes : List (K × Option V)) (key : K)
    (h : left key = right key) : applyWrites left writes key = applyWrites right writes key := by
  induction writes generalizing left right with
  | nil => exact h
  | cons entry rest ih =>
    apply ih
    by_cases he : key = entry.1 <;> simp [he, h]

theorem written_key_independent_of_initial_state {K V : Type} [DecidableEq K]
    (left right : K → Option V) (writes : List (K × Option V)) (key : K)
    (h : ∃ value, (key, value) ∈ writes) :
    applyWrites left writes key = applyWrites right writes key := by
  induction writes generalizing left right with
  | nil => simp at h
  | cons entry rest ih =>
    rcases h with ⟨value, hm⟩
    simp only [List.mem_cons] at hm
    rcases hm with he | hr
    · have hk : key = entry.1 := congrArg Prod.fst he
      simp only [applyWrites]
      apply apply_writes_locality
      simp [hk]
    · exact ih _ _ ⟨value, hr⟩

/-- Replaying a complete write list, including deletions, changes no values. -/
theorem apply_writes_idempotent {K V : Type} [DecidableEq K]
    (state : K → Option V) (writes : List (K × Option V)) :
    applyWrites (applyWrites state writes) writes = applyWrites state writes := by
  funext key
  by_cases h : ∃ value, (key, value) ∈ writes
  · exact written_key_independent_of_initial_state _ _ _ _ h
  · have hu : ∀ entry ∈ writes, entry.1 ≠ key := by
      intro entry hm he
      apply h
      have hp : (key, entry.2) = entry := Prod.ext he.symm rfl
      exact ⟨entry.2, by rw [hp]; exact hm⟩
    exact untouched_key_preserved _ _ _ hu

/-- Independent writes may be reordered; writes to the same key retain order. -/
theorem distinct_writes_commute {K V : Type} [DecidableEq K]
    (state : K → Option V) (a b : K) (va vb : Option V) (h : a ≠ b) :
    applyWrites state [(a, va), (b, vb)] = applyWrites state [(b, vb), (a, va)] := by
  funext key
  by_cases ha : key = a
  · subst key
    simp [applyWrites, h]
  · by_cases hb : key = b
    · subst key
      simp [applyWrites, ha]
    · simp [applyWrites, ha, hb]

/-- A deletion at the end removes the key regardless of its prior history. -/
theorem last_delete_removes_key {K V : Type} [DecidableEq K]
    (state : K → Option V) (earlier : List (K × Option V)) (key : K) :
    applyWrites state (earlier ++ [(key, none)]) key = none :=
  last_write_wins state earlier key none

end KasaneEvm
