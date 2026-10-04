import KasaneEvm.Ledger

namespace KasaneEvm.LedgerSaturation
open KasaneEvm.Ledger

def maxTime : Nat := 2^64 - 1

/-- The pinned TimeStamp::add uses saturating_add after checked duration narrowing. -/
def addTime (a duration : Nat) : Nat := min (a + duration) maxTime

def admit (created now window drift : Nat) (seen : Bool) : Admission :=
  if addTime created window < now then .tooOld
  else if addTime now drift < created then .future
  else if seen then .duplicate else .apply

theorem addTime_bound (a duration : Nat) : addTime a duration ≤ maxTime := by
  unfold addTime
  exact Nat.min_le_right _ _

theorem old_iff (created now window : Nat) (clock : now ≤ maxTime) :
    addTime created window < now ↔ created + window < now := by
  unfold addTime
  omega

theorem future_iff (created now drift : Nat) (timestamp : created ≤ maxTime) :
    addTime now drift < created ↔ now + drift < created := by
  unfold addTime
  omega

theorem admission_eq (created now window drift : Nat) (seen : Bool)
    (timestamp : created ≤ maxTime) (clock : now ≤ maxTime) :
    admit created now window drift seen = KasaneEvm.Ledger.admit created now window drift seen := by
  simp only [admit, KasaneEvm.Ledger.admit,
    old_iff created now window clock, future_iff created now drift timestamp]

theorem nested_add (a window drift : Nat) :
    addTime (addTime a window) drift = addTime a (window + drift) := by
  unfold addTime
  omega

theorem retention_iff (accepted now window drift : Nat) (clock : now ≤ maxTime) :
    now ≤ addTime (addTime accepted window) drift ↔ now ≤ accepted + window + drift := by
  rw [nested_add]
  unfold addTime
  omega

/-- Same fixed-hash abstraction as Ledger.submit, with the actual saturating time comparisons. -/
def submit (created now window drift : Nat) (state : DedupState) : DedupState :=
  let retained := state.acceptedAt.filter
    (fun accepted => now ≤ addTime (addTime accepted window) drift)
  match admit created now window drift retained.isSome with
  | .apply => ⟨some now, state.effects + 1⟩
  | _ => ⟨retained, state.effects⟩

theorem submit_eq (created now window drift : Nat) (state : DedupState)
    (timestamp : created ≤ maxTime) (clock : now ≤ maxTime) :
    submit created now window drift state = KasaneEvm.Ledger.submit created now window drift state := by
  have hfilter : state.acceptedAt.filter
      (fun accepted => now ≤ addTime (addTime accepted window) drift) =
      state.acceptedAt.filter (fun accepted => now ≤ accepted + window + drift) := by
    cases he : state.acceptedAt with
    | none => rfl
    | some accepted => simp [Option.filter, retention_iff accepted now window drift clock]
  simp only [submit, KasaneEvm.Ledger.submit, hfilter,
    admission_eq created now window drift _ timestamp clock]
  rfl

def trace (created window drift : Nat) : List Nat → DedupState → DedupState
  | [], state => state
  | now :: rest, state => trace created window drift rest (submit created now window drift state)

theorem trace_eq (created window drift : Nat) (times : List Nat) (state : DedupState)
    (timestamp : created ≤ maxTime) (clocks : ∀ now ∈ times, now ≤ maxTime) :
    trace created window drift times state = KasaneEvm.Ledger.trace created window drift times state := by
  induction times generalizing state with
  | nil => rfl
  | cons now rest ih =>
    simp only [trace, KasaneEvm.Ledger.trace]
    rw [submit_eq created now window drift state timestamp (clocks now (by simp))]
    exact ih _ (fun now hn => clocks now (by simp [hn]))

theorem arbitrary_retries_have_one_effect (created window drift lower : Nat)
    (times : List Nat) (state : DedupState)
    (timestamp : created ≤ maxTime) (clocks : ∀ now ∈ times, now ≤ maxTime)
    (safe : Safe created window drift lower state)
    (monotone : MonotoneFrom lower times) :
    (trace created window drift times state).effects = 1 := by
  rw [trace_eq created window drift times state timestamp clocks]
  exact KasaneEvm.Ledger.arbitrary_retries_have_one_effect created window drift lower times state safe monotone

end KasaneEvm.LedgerSaturation
