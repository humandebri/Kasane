import StdUnsafeCellCorrespondence
namespace StdCellGetState
abbrev Pointer := StdMemReplaceState.Pointer
abbrev Heap := StdMemReplaceState.Heap
abbrev Scalar := StdMemReplaceState.Scalar
abbrev Permissions := StdMemReplaceState.Permissions
abbrev Value := StdMemReplaceState.Value
abbrev PointerOutcome := StdUnsafeCellPointer.Outcome
inductive Fault where
  | invalidLocal | noReturn | deniedRead | uninitialized
  | pointerFailure (reason : StdUnsafeCellPointer.Fault)
inductive Outcome where
  | returned (value : Scalar) (heap : Heap)
  | fault (reason : Fault) (heap : Heap)
structure API where
  project : Pointer → Heap → PointerOutcome
  unsafeGet : Pointer → Heap → PointerOutcome
  read : Pointer → Heap → Outcome
inductive Instruction where
  | live (dst : Nat)
  | dead (src : Nat)
  | project (dst src : Nat)
  | unsafeGet (dst src : Nat)
  | read (dst src : Nat)
  | ret
abbrev Locals := Nat → Option Value
def put (s : Locals) (i : Nat) (v : Option Value) : Locals := fun j => if j = i then v else s j
def initial (p : Pointer) : Locals := fun i => if i = 1 then some (.pointer p) else none
-- Pointer faults include the effect's current heap. This is an IR observation
-- abstraction, not a Rust unwind/UB theorem. Exact source cleanup is audited.
def bindPointer (result : PointerOutcome) (next : Pointer → Heap → Outcome) : Outcome :=
  match result with
  | .returned p h => next p h
  | .fault reason h => .fault (.pointerFailure reason) h

def execute (api : API) : List Instruction → Locals → Heap → Outcome
  | [], _, h => .fault .noReturn h
  | .live i :: tail, s, h => execute api tail (put s i none) h
  | .dead i :: tail, s, h => execute api tail (put s i none) h
  | .project dst src :: tail, s, h => match s src with
    | some (.pointer p) => bindPointer (api.project p h)
      (fun p h => execute api tail (put s dst (some (.pointer p))) h)
    | _ => .fault .invalidLocal h
  | .unsafeGet dst src :: tail, s, h => match s src with
    | some (.pointer p) => bindPointer (api.unsafeGet p h)
      (fun p h => execute api tail (put (put s src none) dst (some (.pointer p))) h)
    | _ => .fault .invalidLocal h
  | .read dst src :: tail, s, h => match s src with
    | some (.pointer p) => match api.read p h with
      | .returned v h => execute api tail (put s dst (some (.scalar v))) h
      | .fault reason h => .fault reason h
    | _ => .fault .invalidLocal h
  | .ret :: _, s, h => match s 0 with
    | some (.scalar v) => .returned v h
    | _ => .fault .invalidLocal h

def readHeap (rights : Permissions) (p : Pointer) (h : Heap) : Outcome :=
  if rights.read p then match h p.address with
    | some v => .returned v h
    | none => .fault .uninitialized h
  else .fault .deniedRead h

-- Linked to the actual extracted UnsafeCell::get sequence, including all its
-- arbitrary address/cast effects; field projection remains an explicit effect.
def linkedAPI (project : Pointer → Heap → PointerOutcome)
    (pointerAPI : StdUnsafeCellPointer.API) (rights : Permissions) : API :=
  ⟨project, fun p h => StdUnsafeCellPointer.execute pointerAPI
    StdUnsafeCellPointer.actualUnsafeCellGet (StdUnsafeCellPointer.initial p) h,
    readHeap rights⟩
end StdCellGetState
