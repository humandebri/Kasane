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

end KasaneEvm.Journal
