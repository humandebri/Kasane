import KasaneEvm.Execution
import KasaneEvm.State
import KasaneEvm.Journal

namespace KasaneEvm.Refinement

open Journal

/-- A correspondence between the two Lean models, not extraction from Rust.
Storage zeros are canonicalized to absent entries; account lifecycle, codecs,
physical stable maps and construction of the actual Rust diff are outside scope. -/
def storageMap (s : World) : Nat → Option Nat := fun key => storageValue (s.storage key)

def forwardWrites : List Action → List (Nat × Option Nat)
  | [] => []
  | .store key value :: rest => (key, storageValue value) :: forwardWrites rest
  | .transfer _ :: rest => forwardWrites rest

def rollbackWrites : List Entry → List (Nat × Option Nat)
  | [] => []
  | .storageChanged key old :: rest => (key, storageValue old) :: rollbackWrites rest
  | .balanceTransfer _ :: rest => rollbackWrites rest

theorem single_storage_write_projection (s : World) (key value : Nat) :
    applyWrites (storageMap s) [(key, storageValue value)] =
      storageMap (step s (.store key value)) := by
  funext k
  by_cases h : k = key <;> simp [applyWrites, storageMap, step, put, h]

/-- A complete forward trace writes exactly the journal's storage observation. -/
theorem forward_storage_projection (s : World) (actions : List Action) :
    applyWrites (storageMap s) (forwardWrites actions) = storageMap (run s actions).1 := by
  induction actions generalizing s with
  | nil => rfl
  | cons a rest ih =>
    cases a with
    | store key value =>
      have he : (fun k => if k = key then storageValue value else storageMap s k) =
          storageMap (step s (.store key value)) :=
        single_storage_write_projection s key value
      simp only [forwardWrites, applyWrites, run]
      rw [he]
      exact ih _
    | transfer amount =>
      simpa [forwardWrites, run, storageMap, step] using ih (step s (.transfer amount))

/-- Reverse journal entries produce the same storage observation as undo. -/
theorem rollback_storage_projection (s : World) (entries : List Entry) :
    applyWrites (storageMap s) (rollbackWrites entries) = storageMap (undo s entries) := by
  induction entries generalizing s with
  | nil => rfl
  | cons entry rest ih =>
    cases entry with
    | storageChanged key old =>
      have he : (fun k => if k = key then storageValue old else storageMap s k) =
          storageMap (undoEntry s (.storageChanged key old)) :=
        single_storage_write_projection s key old
      simp only [rollbackWrites, applyWrites, undo]
      rw [he]
      exact ih _
    | balanceTransfer amount =>
      simpa [rollbackWrites, undo, storageMap, undoEntry] using
        ih (undoEntry s (.balanceTransfer amount))

/-- This closes the rollback assumption for the modeled storage write list. -/
theorem forward_then_rollback_restores_storage (s : World) (actions : List Action)
    (h : ValidTrace s actions) :
    applyWrites (applyWrites (storageMap s) (forwardWrites actions))
      (rollbackWrites (run s actions).2) = storageMap s := by
  rw [forward_storage_projection, rollback_storage_projection, rollback_trace s actions h]

theorem committed_execution_storage_matches_journal (s : World) (actions : List Action)
    (gas price : Nat) (output : Bytes) (address : Option Nat) (logs : List Bytes) :
    finish applyWrites (storageMap s) (forwardWrites actions)
      (.success gas output address logs) price false true =
      (storageMap (run s actions).1, .ok (receipt (.success gas output address logs) price)) := by
  simp [finish, forward_storage_projection]

/-- A modeled speculative trace, then its undo writes, restores every storage
key while returning the REVERT receipt. Real Rust diff construction is unproved. -/
theorem reverted_execution_restores_storage (s : World) (actions : List Action)
    (h : ValidTrace s actions) (gas price : Nat) (output : Bytes) :
    finish applyWrites (applyWrites (storageMap s) (forwardWrites actions))
      (rollbackWrites (run s actions).2) (.revert gas output) price false true =
      (storageMap s, .ok (receipt (.revert gas output) price)) := by
  simp [finish, forward_then_rollback_restores_storage s actions h]

end KasaneEvm.Refinement
