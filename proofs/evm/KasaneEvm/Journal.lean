import KasaneEvm.Fees

namespace KasaneEvm.Journal

/-- Projection of two distinct, loaded, existing accounts. Lifecycle flags,
warmth, code, transient storage, and host failures are outside this model. -/
structure World where
  storage : Nat → Nat
  source : Nat
  target : Nat

def put (storage : Nat → Nat) (key value : Nat) : Nat → Nat :=
  fun k => if k = key then value else storage k

inductive Action where
  | store (key value : Nat)
  | transfer (amount : Nat)
  deriving Repr

inductive Entry where
  | storageChanged (key old : Nat)
  | balanceTransfer (amount : Nat)
  deriving Repr

def valid (s : World) : Action → Prop
  | .store _ value => value ≤ max256
  | .transfer amount => amount ≤ s.source ∧ s.target + amount ≤ max256

def step (s : World) : Action → World
  | .store key value => { s with storage := put s.storage key value }
  | .transfer amount => { s with source := s.source - amount, target := s.target + amount }

def save (s : World) : Action → Entry
  | .store key _ => .storageChanged key (s.storage key)
  | .transfer amount => .balanceTransfer amount

def undoEntry (s : World) : Entry → World
  | .storageChanged key old => { s with storage := put s.storage key old }
  | .balanceTransfer amount => { s with source := s.source + amount, target := s.target - amount }

/-- Newest-first representation of Rust's reversed journal suffix. -/
def undo (s : World) : List Entry → World
  | [] => s
  | e :: es => undo (undoEntry s e) es

def run (s : World) : List Action → World × List Entry
  | [] => (s, [])
  | a :: rest =>
      let later := run (step s a) rest
      (later.1, later.2 ++ [save s a])

def ValidTrace (s : World) : List Action → Prop
  | [] => True
  | a :: rest => valid s a ∧ ValidTrace (step s a) rest

theorem restore_storage (slots : Nat → Nat) (key value : Nat) :
    put (put slots key value) key (slots key) = slots := by
  funext k
  by_cases h : k = key <;> simp [put, h]

theorem step_inverse (s : World) (a : Action) (h : valid s a) :
    undoEntry (step s a) (save s a) = s := by
  cases a with
  | store key value =>
    simp only [step, save, undoEntry, restore_storage]
  | transfer amount =>
    cases s with
    | mk slots source target =>
      simp only [valid] at h
      simp only [step, save, undoEntry, World.mk.injEq]
      exact ⟨True.intro, by omega, by omega⟩

theorem undo_append (s : World) (a b : List Entry) :
    undo s (a ++ b) = undo (undo s a) b := by
  induction a generalizing s with
  | nil => rfl
  | cons e es ih => exact ih (undoEntry s e)

/-- For every finite valid trace, reverse journal replay restores all modeled
storage slots and both balances. The inverse is proved, not assumed. -/
theorem rollback_trace (s : World) (actions : List Action) (h : ValidTrace s actions) :
    undo (run s actions).1 (run s actions).2 = s := by
  induction actions generalizing s with
  | nil => rfl
  | cons a rest ih =>
    simp only [ValidTrace] at h
    simp only [run, undo_append]
    rw [ih (step s a) h.2]
    exact step_inverse s a h.1

/-- Child commit retains its journal entries; a parent rollback undoes both
the committed child suffix and the parent's earlier suffix. -/
theorem parent_revert_after_child_commit (s : World) (parent child : List Action)
    (hp : ValidTrace s parent) (hc : ValidTrace (run s parent).1 child) :
    undo (run (run s parent).1 child).1
      ((run (run s parent).1 child).2 ++ (run s parent).2) = s := by
  rw [undo_append, rollback_trace _ _ hc, rollback_trace _ _ hp]

theorem child_revert_preserves_parent (s : World) (parent child : List Action)
    (hc : ValidTrace (run s parent).1 child) :
    undo (run (run s parent).1 child).1 (run (run s parent).1 child).2 =
      (run s parent).1 := rollback_trace _ _ hc

theorem rollback_logs (before emitted : List Nat) :
    (before ++ emitted).take before.length = before := by simp

/-- The index layout used by `JournalInner::checkpoint_revert`. Journal entries
are oldest-first here, as in Rust; `undo` consumes the drained suffix in reverse. -/
structure Checkpoint where
  journal_i : Nat
  log_i : Nat

def revertAt (s : World) (entries : List Entry) (logs : List Nat)
    (cp : Checkpoint) : World × List Entry × List Nat :=
  (undo s ((entries.drop cp.journal_i).reverse),
    entries.take cp.journal_i, logs.take cp.log_i)

theorem revert_at_checkpoint (s : World) (prior : List Entry)
    (beforeLogs emitted : List Nat) (actions : List Action)
    (h : ValidTrace s actions) :
    revertAt (run s actions).1 (prior ++ (run s actions).2.reverse)
      (beforeLogs ++ emitted) ⟨prior.length, beforeLogs.length⟩ =
      (s, prior, beforeLogs) := by
  simp [revertAt, rollback_trace s actions h]

theorem commit_keeps_undo_entries (entries : List Entry) (depth : Nat) :
    (entries, depth - 1).1 = entries := rfl

/-- Transaction-level nonce/fee observations are outside the CALL checkpoint.
This models their position; the Rust handler ordering is checked separately. -/
structure Transaction where
  world : World
  nonce : Nat
  paidFee : Nat

def rollbackCall (tx : Transaction) (entries : List Entry) : Transaction :=
  { tx with world := undo tx.world entries }

theorem call_rollback_preserves_transaction_accounting (tx : Transaction) (entries : List Entry) :
    (rollbackCall tx entries).nonce = tx.nonce ∧
    (rollbackCall tx entries).paidFee = tx.paidFee := ⟨rfl, rfl⟩

/-- These bounds describe only the two-account/storage projection. -/
def Bounded (s : World) : Prop :=
  s.source ≤ max256 ∧ s.target ≤ max256 ∧ ∀ key, s.storage key ≤ max256

theorem step_preserves_bounds (s : World) (a : Action) (hs : Bounded s) (ha : valid s a) :
    Bounded (step s a) := by
  cases a with
  | store key value =>
    refine ⟨hs.1, hs.2.1, ?_⟩
    intro k
    by_cases h : k = key
    · simpa [step, put, h] using ha
    · simpa [step, put, h] using hs.2.2 k
  | transfer amount =>
    exact ⟨Nat.le_trans (Nat.sub_le _ _) hs.1, ha.2, hs.2.2⟩

theorem step_conserves_balance (s : World) (a : Action) (ha : valid s a) :
    (step s a).source + (step s a).target = s.source + s.target := by
  cases a with
  | store key value => rfl
  | transfer amount =>
    simp only [valid] at ha
    simp only [step]
    omega

theorem trace_preserves_bounds (s : World) (actions : List Action)
    (hs : Bounded s) (h : ValidTrace s actions) : Bounded (run s actions).1 := by
  induction actions generalizing s with
  | nil => exact hs
  | cons a rest ih =>
    exact ih (step s a) (step_preserves_bounds s a hs h.1) h.2

theorem trace_conserves_balance (s : World) (actions : List Action)
    (h : ValidTrace s actions) :
    (run s actions).1.source + (run s actions).1.target = s.source + s.target := by
  induction actions generalizing s with
  | nil => rfl
  | cons a rest ih =>
    exact (ih (step s a) h.2).trans (step_conserves_balance s a h.1)

theorem run_append (s : World) (first second : List Action) :
    run s (first ++ second) =
      ((run (run s first).1 second).1, (run (run s first).1 second).2 ++ (run s first).2) := by
  induction first generalizing s with
  | nil => simp [run]
  | cons a rest ih => simp [run, ih, List.append_assoc]

theorem valid_trace_append_iff (s : World) (first second : List Action) :
    ValidTrace s (first ++ second) ↔
      ValidTrace s first ∧ ValidTrace (run s first).1 second := by
  induction first generalizing s with
  | nil => simp [ValidTrace, run]
  | cons a rest ih => simp [ValidTrace, run, ih, and_assoc]

/-- Unbounded finite nesting is represented by concatenating valid frame traces.
No Rust stack limit, CREATE or SELFDESTRUCT semantics are inferred. -/
theorem rollback_nested_frames (s : World) (frames : List (List Action))
    (h : ValidTrace s frames.flatten) :
    undo (run s frames.flatten).1 (run s frames.flatten).2 = s :=
  rollback_trace s frames.flatten h

theorem trace_preserves_unwritten_storage (s : World) (actions : List Action) (key : Nat)
    (h : ∀ value, Action.store key value ∉ actions) :
    (run s actions).1.storage key = s.storage key := by
  induction actions generalizing s with
  | nil => rfl
  | cons a rest ih =>
    have hr : ∀ value, Action.store key value ∉ rest := by
      intro value hm
      exact h value (by simp [hm])
    rw [show (run s (a :: rest)).1 = (run (step s a) rest).1 from rfl, ih _ hr]
    cases a with
    | transfer amount => rfl
    | store k value =>
      have hk : key ≠ k := by
        intro he
        subst k
        exact h value (by simp)
      simp [step, put, hk]

/-- Re-truncating the projected state/history/logs is inert. Depth accounting is
not modeled: this does not license reuse of a consumed Rust checkpoint. -/
theorem checkpoint_revert_idempotent (s : World) (entries : List Entry) (logs : List Nat)
    (cp : Checkpoint) :
    let once := revertAt s entries logs cp
    revertAt once.1 once.2.1 once.2.2 cp = once := by
  simp [revertAt, undo, List.take_take]

theorem rollback_valid_call_restores_world (tx : Transaction) (actions : List Action)
    (h : ValidTrace tx.world actions) :
    rollbackCall { tx with world := (run tx.world actions).1 } (run tx.world actions).2 = tx := by
  simp [rollbackCall, rollback_trace tx.world actions h]

theorem run_journal_length (s : World) (actions : List Action) :
    (run s actions).2.length = actions.length := by
  induction actions generalizing s with
  | nil => rfl
  | cons a rest ih => simp [run, ih]

end KasaneEvm.Journal
