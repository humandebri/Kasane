import Lean

/- Fragment model for the unapplied NoRetag copy/reborrow candidate.
   Source: fixed Charon expressions.rs, Rvalue::Use/Operand::Copy/WithRetag.
   Locals contain pointer identity and metadata. Only NoRetag copies are modeled.
   The borrow operation is universally quantified, including failure and arbitrary
   world effects; no properties of that operation are assumed.
   This is NOT a proof of Rust/LLBC semantics, lifetimes, or the OCaml pass.
-/
namespace RefCopyFragment

structure Pointer where
  address : Nat
  metadata : Nat
  deriving DecidableEq

abbrev Locals := Nat → Pointer

def write (locals : Locals) (id : Nat) (pointer : Pointer) : Locals :=
  fun j => if j = id then pointer else locals j

inductive Metadata where
  | constant (value : Nat)
  | fromPointer

def metadataValue : Metadata → Pointer → Nat
  | .constant n, _ => n
  | .fromPointer, p => p.metadata

/-- The observed state hides only the dead temporary local. -/
def observe {World : Type} (temporary : Nat) (locals : Locals) (world : World) :=
  ((fun j => if j = temporary then none else some (locals j)), world)

/-- Original fragment: copy without retag, then reborrow from the temporary. -/
def original {World Fault : Type} (locals : Locals) (world : World)
    (source temporary destination : Nat) (metadata : Metadata)
    (project : Pointer → World → Pointer)
    (borrow : Pointer → Nat → World → Except Fault (Pointer × World)) := do
  let pointer := locals source
  let copied := write locals temporary pointer
  let outcome ← borrow (project (copied temporary) world)
    (metadataValue metadata (copied temporary)) world
  pure (observe temporary (write copied destination outcome.1) outcome.2)

/-- Candidate fragment: reborrow the same source directly. -/
def fused {World Fault : Type} (locals : Locals) (world : World)
    (source temporary destination : Nat) (metadata : Metadata)
    (project : Pointer → World → Pointer)
    (borrow : Pointer → Nat → World → Except Fault (Pointer × World)) := do
  let pointer := locals source
  let outcome ← borrow (project pointer world) (metadataValue metadata pointer) world
  pure (observe temporary (write locals destination outcome.1) outcome.2)

theorem write_same (locals : Locals) (id : Nat) (pointer : Pointer) :
    write locals id pointer id = pointer := by simp [write]

theorem write_other (locals : Locals) (id j : Nat) (pointer : Pointer)
    (h : j ≠ id) : write locals id pointer j = locals j := by simp [write, h]

/-- Local copy is unobservable after the temporary's last use, even when the
    later destination has the same id. Heap/tag effects remain in World. -/
theorem dead_copy_observation {World : Type} (locals : Locals) (world : World)
    (temporary destination : Nat) (copied result : Pointer) :
    observe temporary (write (write locals temporary copied) destination result) world =
      observe temporary (write locals destination result) world := by
  apply Prod.ext
  · funext j
    by_cases ht : j = temporary
    · simp [observe, ht]
    · by_cases hd : j = destination <;> simp [observe, write, ht, hd]
  · rfl

/-- For every pointer identity/metadata, projection, borrow implementation,
    world state, and error outcome, this fragment's observation is unchanged.
    The theorem is conditional on the fragment definitions above representing
    the LLBC operations; this correspondence is not yet established. -/
theorem fragment_all_inputs {World Fault : Type} (locals : Locals) (world : World)
    (source temporary destination : Nat) (metadata : Metadata)
    (project : Pointer → World → Pointer)
    (borrow : Pointer → Nat → World → Except Fault (Pointer × World)) :
    original locals world source temporary destination metadata project borrow =
      fused locals world source temporary destination metadata project borrow := by
  simp only [original, fused, write_same]
  cases h : borrow (project (locals source) world)
      (metadataValue metadata (locals source)) world with
  | error fault => rfl
  | ok outcome =>
    change Except.ok (observe temporary
      (write (write locals temporary (locals source)) destination outcome.1) outcome.2) =
      Except.ok (observe temporary (write locals destination outcome.1) outcome.2)
    congr 1
    exact dead_copy_observation locals outcome.2 temporary destination
      (locals source) outcome.1

end RefCopyFragment
