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
  else if !sizesValid then (before, .error .resultTooLarge)
  else (commit before diff, .ok (receipt r price))

theorem precommit_stop_preserves_state {S D : Type} (commit : S → D → S)
    (before : S) (diff : D) (r : Result) (price : Nat) (sizesValid : Bool) :
    (finish commit before diff r price true sizesValid).1 = before := by
  rfl

theorem size_error_preserves_state {S D : Type} (commit : S → D → S)
    (before : S) (diff : D) (r : Result) (price : Nat) :
    finish commit before diff r price false false =
      (before, .error .resultTooLarge) := by
  rfl

/-- Every error represented by this model precedes the supplied DB commit.
This does not assert that arbitrary host/precompile side effects are absent. -/
theorem finish_error_preserves_state {S D : Type} (commit : S → D → S)
    (before : S) (diff : D) (r : Result) (price : Nat) (stopped sizesValid : Bool)
    (error : Error) (h : (finish commit before diff r price stopped sizesValid).2 = .error error) :
    (finish commit before diff r price stopped sizesValid).1 = before := by
  cases stopped <;> cases sizesValid <;> simp_all [finish]

theorem finish_success_iff {S D : Type} (commit : S → D → S)
    (before : S) (diff : D) (r : Result) (price : Nat) (stopped sizesValid : Bool) :
    (finish commit before diff r price stopped sizesValid).2 = .ok (receipt r price) ↔
      stopped = false ∧ sizesValid = true := by
  cases stopped <;> cases sizesValid <;> simp [finish]

theorem finish_success_commits {S D : Type} (commit : S → D → S)
    (before : S) (diff : D) (r : Result) (price : Nat) :
    finish commit before diff r price false true =
      (commit before diff, .ok (receipt r price)) := by rfl

/-- A rejected attempt cannot change the input state of a subsequent attempt. -/
theorem rejected_attempt_retry {S D : Type} (commit : S → D → S)
    (before : S) (rejected next : D) (r nextResult : Result) (price nextPrice : Nat)
    (stopped sizesValid nextStopped nextSizesValid : Bool) (error : Error)
    (h : (finish commit before rejected r price stopped sizesValid).2 = .error error) :
    finish commit (finish commit before rejected r price stopped sizesValid).1
      next nextResult nextPrice nextStopped nextSizesValid =
      finish commit before next nextResult nextPrice nextStopped nextSizesValid := by
  rw [finish_error_preserves_state _ _ _ _ _ _ _ _ h]

/-- Explicit refinement obligation on the supplied diff/commit. Revert does not
itself imply this fact; revm rollback and the DB adapter must establish it. -/
theorem revert_preserves_contract_observation {S D O : Type}
    (commit : S → D → S) (observeContract : S → O) (before : S) (diff : D)
    (gas price : Nat) (output : Bytes) (sizesValid : Bool)
    (rollback : observeContract (commit before diff) = observeContract before) :
    observeContract (finish commit before diff (.revert gas output) price false sizesValid).1 =
      observeContract before := by
  cases sizesValid
  · rfl
  · exact rollback

theorem unsuccessful_receipt_has_no_logs (r : Result) (price : Nat)
    (h : (receipt r price).status = 0) :
    (receipt r price).logs = [] ∧ (receipt r price).address = none := by
  cases r <;> simp_all [receipt]

theorem receipt_status_binary (r : Result) (price : Nat) :
    (receipt r price).status = 0 ∨ (receipt r price).status = 1 := by
  cases r <;> simp [receipt]

/-- Size limits are parameters: the adapter supplies the actual constants and
byte lengths. This models all four checks, not byte decoding or allocation. -/
structure LogSize where
  topics : Nat
  data : Nat
  deriving Repr

structure SizeLimits where
  output : Nat
  logs : Nat
  topics : Nat
  data : Nat

def resultSizesValid (limits : SizeLimits) (output : Nat) (logs : List LogSize) : Bool :=
  decide (output ≤ limits.output ∧ logs.length ≤ limits.logs ∧
    ∀ log ∈ logs, log.topics ≤ limits.topics ∧ log.data ≤ limits.data)

theorem result_sizes_valid_iff (limits : SizeLimits) (output : Nat) (logs : List LogSize) :
    resultSizesValid limits output logs = true ↔
      output ≤ limits.output ∧ logs.length ≤ limits.logs ∧
        ∀ log ∈ logs, log.topics ≤ limits.topics ∧ log.data ≤ limits.data := by
  simp [resultSizesValid]

theorem invalid_result_sizes_preserve_state {S D : Type} (commit : S → D → S)
    (before : S) (diff : D) (r : Result) (price output : Nat)
    (limits : SizeLimits) (logs : List LogSize)
    (h : ¬ (output ≤ limits.output ∧ logs.length ≤ limits.logs ∧
      ∀ log ∈ logs, log.topics ≤ limits.topics ∧ log.data ≤ limits.data)) :
    finish commit before diff r price false (resultSizesValid limits output logs) =
      (before, .error .resultTooLarge) := by
  simp [resultSizesValid, h, finish]

theorem result_sizes_monotone (small large : SizeLimits) (output : Nat) (logs : List LogSize)
    (ho : small.output ≤ large.output) (hl : small.logs ≤ large.logs)
    (ht : small.topics ≤ large.topics) (hd : small.data ≤ large.data)
    (h : resultSizesValid small output logs = true) :
    resultSizesValid large output logs = true := by
  rw [result_sizes_valid_iff] at h ⊢
  exact ⟨Nat.le_trans h.1 ho, Nat.le_trans h.2.1 hl,
    fun log hm => ⟨Nat.le_trans (h.2.2 log hm).1 ht, Nat.le_trans (h.2.2 log hm).2 hd⟩⟩

end KasaneEvm
