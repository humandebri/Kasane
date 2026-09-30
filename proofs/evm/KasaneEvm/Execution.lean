import KasaneEvm.Fees

namespace KasaneEvm

abbrev Bytes := List UInt8

/-- Addresses, logs, and halt reasons are opaque here; codecs are outside scope. -/
inductive Result where
  | success (gas : Nat) (output : Bytes) (address : Option Nat) (logs : List Bytes)
  | revert (gas : Nat) (output : Bytes)
  | halt (gas : Nat) (reason : Nat)
  deriving Repr

def Result.gas : Result → Nat
  | .success gas _ _ _ | .revert gas _ | .halt gas _ => gas

structure Receipt where
  status : Nat
  gas : Nat
  output : Bytes
  address : Option Nat
  logs : List Bytes
  fee : Nat
  deriving Repr

def receipt (r : Result) (price : Nat) : Receipt :=
  match r with
  | .success gas output address logs => ⟨1, gas, output, address, logs, totalFee gas price 0 0⟩
  | .revert gas output => ⟨0, gas, output, none, [], totalFee gas price 0 0⟩
  | .halt gas _ => ⟨0, gas, [], none, [], totalFee gas price 0 0⟩

theorem receipt_gas_matches (r : Result) (price : Nat) : (receipt r price).gas = r.gas := by
  cases r <;> rfl

theorem success_receipt (gas price : Nat) (output : Bytes) (address : Option Nat)
    (logs : List Bytes) :
    (receipt (.success gas output address logs) price).status = 1 ∧
    (receipt (.success gas output address logs) price).output = output ∧
    (receipt (.success gas output address logs) price).address = address ∧
    (receipt (.success gas output address logs) price).logs = logs := by
  exact ⟨rfl, rfl, rfl, rfl⟩

theorem receipt_fee_matches (r : Result) (price : Nat)
    (hg : r.gas ≤ max64) (hp : price ≤ max64) :
    (receipt r price).fee = r.gas * price := by
  cases r <;> exact execution_fee_exact _ _ hg hp

theorem revert_receipt (gas price : Nat) (output : Bytes) :
    (receipt (.revert gas output) price).status = 0 ∧
    (receipt (.revert gas output) price).logs = [] ∧
    (receipt (.revert gas output) price).address = none ∧
    (receipt (.revert gas output) price).output = output := by
  exact ⟨rfl, rfl, rfl, rfl⟩

theorem halt_receipt (gas reason price : Nat) :
    (receipt (.halt gas reason) price).status = 0 ∧
    (receipt (.halt gas reason) price).logs = [] ∧
    (receipt (.halt gas reason) price).output = [] := by
  exact ⟨rfl, rfl, rfl⟩

inductive Error where
  | preCommit | resultTooLarge
  deriving DecidableEq, Repr

/-- `state` denotes the supplied DB, possibly a cache. `commit` includes the
base-fee credit and diff application. No IC message rollback is assumed. -/
def finish {S D : Type} (commit : S → D → S) (before : S) (diff : D)
    (r : Result) (price : Nat) (stopped sizesValid : Bool) : S × Except Error Receipt :=
  if stopped then (before, .error .preCommit)
  else
    let after := commit before diff
    if sizesValid then (after, .ok (receipt r price))
    else (after, .error .resultTooLarge)

theorem precommit_stop_preserves_state {S D : Type} (commit : S → D → S)
    (before : S) (diff : D) (r : Result) (price : Nat) (sizesValid : Bool) :
    (finish commit before diff r price true sizesValid).1 = before := by
  rfl

theorem size_error_after_commit {S D : Type} (commit : S → D → S)
    (before : S) (diff : D) (r : Result) (price : Nat) :
    finish commit before diff r price false false =
      (commit before diff, .error .resultTooLarge) := by
  rfl

/-- Explicit refinement obligation on the supplied diff/commit. Revert does not
itself imply this fact; revm rollback and the DB adapter must establish it. -/
theorem revert_preserves_contract_observation {S D O : Type}
    (commit : S → D → S) (observeContract : S → O) (before : S) (diff : D)
    (gas price : Nat) (output : Bytes) (sizesValid : Bool)
    (rollback : observeContract (commit before diff) = observeContract before) :
    observeContract (finish commit before diff (.revert gas output) price false sizesValid).1 =
      observeContract before := by
  cases sizesValid <;> exact rollback

end KasaneEvm
