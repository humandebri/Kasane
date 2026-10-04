import StdMemReplaceState

/- Pointer IR for the scalar instance of actual UnsafeCell::get.
   Both address exposure and typed casts are arbitrary stateful effects, including
   changed addresses/tags/heaps and failure. Their physical Rust meaning remains
   unproved. Pointer copying is identity in this IR; repr/layout auditing alone
   does not prove that rule or the validity of the source shared reference.
-/
namespace StdUnsafeCellPointer
abbrev Pointer := StdMemReplaceState.Pointer
abbrev Heap := StdMemReplaceState.Heap
inductive Cast where | cellToScalar | constToMutable
inductive Fault where | invalidLocal | noReturn | externalFailure
inductive Outcome where
  | returned (pointer : Pointer) (heap : Heap)
  | fault (reason : Fault) (heap : Heap)
structure API where
  address : Pointer → Heap → Outcome
  cast : Cast → Pointer → Heap → Outcome
inductive Instruction where
  | live (dst : Nat)
  | dead (src : Nat)
  | address (dst src : Nat)
  | copy (dst src : Nat)
  | cast (kind : Cast) (dst src : Nat)
  | ret
abbrev Locals := Nat → Option Pointer
def put (s : Locals) (i : Nat) (v : Option Pointer) : Locals :=
  fun j => if j = i then v else s j
def initial (p : Pointer) : Locals := fun i => if i = 1 then some p else none
def bind (result : Outcome) (next : Pointer → Heap → Outcome) : Outcome :=
  match result with
  | .returned p h => next p h
  | .fault e h => .fault e h

def execute (api : API) : List Instruction → Locals → Heap → Outcome
  | [], _, h => .fault .noReturn h
  | .live i :: tail, s, h => execute api tail (put s i none) h
  | .dead i :: tail, s, h => execute api tail (put s i none) h
  | .address dst src :: tail, s, h => match s src with
    | some p => bind (api.address p h) (fun p h => execute api tail (put s dst (some p)) h)
    | none => .fault .invalidLocal h
  | .copy dst src :: tail, s, h => match s src with
    | some p => execute api tail (put s dst (some p)) h
    | none => .fault .invalidLocal h
  | .cast kind dst src :: tail, s, h => match s src with
    | some p => bind (api.cast kind p h)
      (fun p h => execute api tail (put (put s src none) dst (some p)) h)
    | none => .fault .invalidLocal h
  | .ret :: _, s, h => match s 0 with
    | some p => .returned p h
    | none => .fault .invalidLocal h

structure IdentityContracts (api : API) : Prop where
  address : ∀ p h, api.address p h = .returned p h
  cast : ∀ kind p h, api.cast kind p h = .returned p h

def identityAPI : API := ⟨Outcome.returned, fun _ => Outcome.returned⟩
end StdUnsafeCellPointer
