import Std

namespace KasaneEvm.Ledger

/-- Decision order in the pinned ledger core after pruning and throttling.
Fee checks precede this layer; balance/allowance checks follow it. -/
inductive Admission where
  | tooOld | future | duplicate | apply
  deriving DecidableEq, Repr

def admit (created now window drift : Nat) (seen : Bool) : Admission :=
  if created + window < now then .tooOld
  else if now + drift < created then .future
  else if seen then .duplicate else .apply

theorem apply_iff (created now window drift : Nat) (seen : Bool) :
    admit created now window drift seen = .apply ↔
      now ≤ created + window ∧ created ≤ now + drift ∧ seen = false := by
  by_cases old : created + window < now <;>
    by_cases future : now + drift < created <;> cases seen <;>
    simp [admit, old, future] <;> omega

theorem retained_key_cannot_apply (created now window drift : Nat) :
    admit created now window drift true ≠ .apply := by
  simp [apply_iff]

/-- Accepted timestamps are bounded by block time plus permitted drift.
The exact purge condition therefore makes a purged retry too old. -/
theorem purged_key_is_too_old (created accepted now window drift : Nat)
    (validAtAcceptance : created ≤ accepted + drift)
    (purged : accepted + window + drift < now) :
    admit created now window drift false = .tooOld := by
  have h : created + window < now := by omega
  simp [admit, h]

structure DedupState where
  acceptedAt : Option Nat
  effects : Nat
  deriving DecidableEq

/-- One fixed transaction hash with a pinned created_at_time. Other ledger
transactions, fees, balances, callback outcomes and message scheduling are
outside this abstraction. Missing timestamps are deliberately excluded. -/
def submit (created now window drift : Nat) (state : DedupState) : DedupState :=
  let retained := state.acceptedAt.filter (fun accepted => now ≤ accepted + window + drift)
  match admit created now window drift retained.isSome with
  | .apply => ⟨some now, state.effects + 1⟩
  | _ => ⟨retained, state.effects⟩

/-- A retained retry may reject for age/future/duplicate, but never creates an effect. -/
theorem retained_retry_preserves_effects (created accepted now window drift effects : Nat)
    (retained : now ≤ accepted + window + drift) :
    (submit created now window drift ⟨some accepted, effects⟩).effects = effects := by
  by_cases old : created + window < now <;>
    by_cases future : now + drift < created <;>
    simp [submit, retained, admit, old, future]

/-- After removal, a retry still cannot create another effect at that time. -/
theorem purged_retry_preserves_effects (created accepted now window drift effects : Nat)
    (valid : created ≤ accepted + drift)
    (purged : accepted + window + drift < now) :
    submit created now window drift ⟨some accepted, effects⟩ = ⟨none, effects⟩ := by
  have hn : ¬ now ≤ accepted + window + drift := by omega
  simp [submit, hn, purged, purged_key_is_too_old created accepted now window drift valid purged]

theorem later_than_purge_rejects (created accepted purgedAt now window drift : Nat)
    (valid : created ≤ accepted + drift)
    (purged : accepted + window + drift < purgedAt)
    (monotoneTime : purgedAt ≤ now) :
    admit created now window drift false = .tooOld := by
  apply purged_key_is_too_old created accepted now window drift valid
  omega

/-- After a successful first transaction: the hash is retained, or it has
already expired. This records the time bound needed across arbitrary retries. -/
def Safe (created window drift lower : Nat) (state : DedupState) : Prop :=
  state.effects = 1 ∧ match state.acceptedAt with
    | some accepted => created ≤ accepted + drift
    | none => created + window < lower

def MonotoneFrom : Nat → List Nat → Prop
  | _, [] => True
  | lower, now :: rest => lower ≤ now ∧ MonotoneFrom now rest

def trace (created window drift : Nat) : List Nat → DedupState → DedupState
  | [], state => state
  | now :: rest, state => trace created window drift rest (submit created now window drift state)

theorem submit_preserves_safe (created window drift lower now : Nat) (state : DedupState)
    (safe : Safe created window drift lower state) (monotone : lower ≤ now) :
    Safe created window drift now (submit created now window drift state) := by
  rcases state with ⟨accepted, effects⟩
  cases accepted with
  | none =>
    have h : created + window < now := by simp [Safe] at safe; omega
    simpa [Safe, submit, admit, h] using And.intro safe.1 h
  | some accepted =>
    have valid : created ≤ accepted + drift := safe.2
    by_cases retained : now ≤ accepted + window + drift
    · have h := retained_retry_preserves_effects created accepted now window drift effects retained
      have hret : (submit created now window drift ⟨some accepted, effects⟩).acceptedAt = some accepted := by
        by_cases old : created + window < now <;>
          by_cases future : now + drift < created <;>
          simp [submit, retained, admit, old, future]
      simp only [Safe, h, hret]
      exact ⟨safe.1, valid⟩
    · have purged : accepted + window + drift < now := by omega
      rw [purged_retry_preserves_effects created accepted now window drift effects valid purged]
      exact ⟨safe.1, by omega⟩

theorem arbitrary_retries_have_one_effect (created window drift lower : Nat)
    (times : List Nat) (state : DedupState)
    (safe : Safe created window drift lower state)
    (monotone : MonotoneFrom lower times) :
    (trace created window drift times state).effects = 1 := by
  induction times generalizing lower state with
  | nil => exact safe.1
  | cons now rest ih =>
    exact ih now (submit created now window drift state)
      (submit_preserves_safe created window drift lower now state safe monotone.1) monotone.2

end KasaneEvm.Ledger
