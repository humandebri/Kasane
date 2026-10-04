import Mathlib.Tactic

/-!
Necessary memory-footprint obligations for the retained array-reference/NonNull cast.
This is a MODEL, not extracted Rust or a proof of LLVM, aliasing, initialization,
concurrency, pointer-to-reference validity, or the OCaml-to-Lean transformer.
Interpreter borrow/shared/loan IDs must be resolved to Rust pointers by an unproved
adapter. In particular, a symbolic value ID denotes content, not an allocation.
-/
namespace SharedArrayPointerFootprint

structure Provenance where
  allocation : Nat
  lower : Nat
  upper : Nat
  live : Bool
  canRead : Bool
  canWrite : Bool
  deriving DecidableEq

structure Pointer where
  address : Nat
  offset : Nat
  provenance : Provenance
  deriving DecidableEq

-- Only necessary non-null, permission and byte-range conditions are modeled.
-- This predicate does not assert that a Rust read or reference creation is safe.
def ReadFootprint (p : Pointer) (bytes : Nat) : Prop :=
  p.address ≠ 0 ∧ (bytes = 0 ∨
    (p.provenance.live = true ∧ p.provenance.canRead = true ∧
     p.provenance.lower ≤ p.offset ∧ p.offset + bytes ≤ p.provenance.upper))

def Aligned (p : Pointer) (alignment : Nat) : Prop :=
  0 < alignment ∧ p.address % alignment = 0

structure SharedArrayView where
  pointer : Pointer
  length : Nat
  elementSize : Nat
  alignment : Nat
  footprint : ReadFootprint pointer (length * elementSize)
  aligned : Aligned pointer alignment

structure NonNullValue where
  pointer : Pointer
  nonzero : pointer.address ≠ 0

-- This constructor is a model definition, not an extraction replacement.
def retain (source : SharedArrayView) : NonNullValue :=
  ⟨source.pointer, source.footprint.1⟩

theorem retain_address (source : SharedArrayView) :
    (retain source).pointer.address = source.pointer.address := rfl

theorem retain_provenance (source : SharedArrayView) :
    (retain source).pointer.provenance = source.pointer.provenance := rfl

theorem retain_offset (source : SharedArrayView) :
    (retain source).pointer.offset = source.pointer.offset := rfl

theorem retain_read_footprint (source : SharedArrayView) (bytes : Nat) :
    ReadFootprint (retain source).pointer bytes ↔ ReadFootprint source.pointer bytes := Iff.rfl

theorem retain_alignment (source : SharedArrayView) :
    Aligned (retain source).pointer source.alignment := source.aligned

theorem retain_write_permission (source : SharedArrayView) :
    (retain source).pointer.provenance.canWrite = source.pointer.provenance.canWrite := rfl

theorem positive_count_first_element_footprint (source : SharedArrayView)
    (hn : 0 < source.length) : ReadFootprint (retain source).pointer source.elementSize := by
  rcases source.footprint with ⟨nonnull, zero | ⟨live, read, lower, upper⟩⟩
  · have sizeZero : source.elementSize = 0 := (Nat.mul_eq_zero.mp zero).resolve_left (Nat.ne_of_gt hn)
    exact ⟨nonnull, Or.inl sizeZero⟩
  · have product : source.elementSize ≤ source.length * source.elementSize := by
      simpa using Nat.mul_le_mul_right source.elementSize (show 1 ≤ source.length by omega)
    exact ⟨nonnull, Or.inr ⟨live, read, lower, by dsimp [retain]; omega⟩⟩

theorem empty_extent_positive_read_rejected (p : Pointer) (bytes : Nat)
    (extent : p.provenance.upper = p.offset) (positive : 0 < bytes) :
    ¬ ReadFootprint p bytes := by
  intro h
  rcases h.2 with zero | ⟨_, _, _, upper⟩
  · omega
  · omega

theorem zero_size_footprint (p : Pointer) (nonnull : p.address ≠ 0) :
    ReadFootprint p 0 := ⟨nonnull, Or.inl rfl⟩

def revokeTemporal (p : Pointer) : Pointer :=
  { p with provenance := { p.provenance with live := false } }

theorem revoke_retains_nonnull_value (value : NonNullValue) :
    (revokeTemporal value.pointer).address ≠ 0 := value.nonzero

theorem revoked_positive_read_rejected (p : Pointer) (bytes : Nat)
    (positive : 0 < bytes) : ¬ ReadFootprint (revokeTemporal p) bytes := by
  intro h
  rcases h.2 with zero | ⟨live, _, _, _⟩
  · omega
  · simp [revokeTemporal] at live

structure InterpreterOrigin where
  borrow : Nat
  sharedBorrow : Nat
  loanSymbolic : Nat
  deriving DecidableEq

-- Mapping this model's IDs to actual Rust allocations is an outstanding obligation.
-- A same-loan alias law must be established by that adapter, not guessed from IDs.
structure OriginResolution where
  resolve : InterpreterOrigin → Option Pointer
  sameLoanAlias : ∀ a b, a.borrow = b.borrow → a.loanSymbolic = b.loanSymbolic →
    resolve a = resolve b

def resolveByBorrow (mapping : Nat → Option Pointer) : OriginResolution where
  resolve origin := mapping origin.borrow
  sameLoanAlias a b borrow _ := by simp [borrow]

theorem same_loan_aliases_resolve_equally (r : OriginResolution) (a b : InterpreterOrigin)
    (borrow : a.borrow = b.borrow) (loan : a.loanSymbolic = b.loanSymbolic) :
    r.resolve a = r.resolve b := r.sameLoanAlias a b borrow loan

private def examplePointer (allocation address : Nat) : Pointer :=
  ⟨address, 0, ⟨allocation, 0, 1, true, true, false⟩⟩

theorem equal_address_does_not_imply_equal_provenance :
    ∃ p q : Pointer, p.address = q.address ∧ p.provenance ≠ q.provenance := by
  exact ⟨revokeTemporal (examplePointer 1 17), examplePointer 2 17, rfl, by decide⟩

theorem equal_symbolic_content_id_does_not_force_equal_address :
    ∃ (a b : InterpreterOrigin) (r : OriginResolution), a.loanSymbolic = b.loanSymbolic ∧
      (r.resolve a).map Pointer.address ≠ (r.resolve b).map Pointer.address := by
  let r := resolveByBorrow (fun id => some (examplePointer id (id + 1)))
  exact ⟨⟨0, 0, 7⟩, ⟨1, 0, 7⟩, r, rfl, by decide⟩

end SharedArrayPointerFootprint
