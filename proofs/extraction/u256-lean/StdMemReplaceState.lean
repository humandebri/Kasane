import Aeneas

/- Scalar-only heap semantics for the actual extracted mem::replace sequence.
   Allocation/offset are separate from pointer tags. Aliases with different tags
   observe the same heap cell. Permissions are explicit inputs, not proved Rust
   lifetime/provenance rules. Raw-address creation preserves the modeled pointer;
   actual RawPtr/WithRetag/Copy/Move correctness is an unproved refinement duty.
   No generic non-Copy ownership, drop, UB, byte layout or concurrency is modeled.
-/
namespace StdMemReplaceState
abbrev Scalar := Aeneas.Std.Isize
structure Address where
  allocation : Nat
  offset : Nat
  deriving DecidableEq
structure Pointer where
  address : Address
  tag : Nat
  deriving DecidableEq
abbrev Heap := Address → Option Scalar
structure Permissions where
  read : Pointer → Bool
  write : Pointer → Bool
inductive Value where
  | pointer (p : Pointer)
  | scalar (v : Scalar)
abbrev Locals := Nat → Option Value
inductive RawKind where | shared | mutable
inductive Instruction where
  | live (dst : Nat)
  | dead (src : Nat)
  | address (kind : RawKind) (dst src : Nat)
  | read (dst ptr : Nat)
  | write (ptr src : Nat)
  | move (dst src : Nat)
  | ret
inductive Fault where | invalidLocal | deniedRead | deniedWrite | uninitialized | noReturn
inductive Outcome where
  | returned (value : Scalar) (heap : Heap)
  | fault (reason : Fault) (heap : Heap)
def put (s : Locals) (i : Nat) (v : Option Value) : Locals :=
  fun j => if j = i then v else s j
def writeHeap (h : Heap) (a : Address) (v : Scalar) : Heap :=
  fun b => if b = a then some v else h b
def initial (p : Pointer) (v : Scalar) : Locals :=
  fun i => if i = 1 then some (.pointer p) else if i = 2 then some (.scalar v) else none

def execute (rights : Permissions) : List Instruction → Locals → Heap → Outcome
  | [], _, h => .fault .noReturn h
  | .live i :: tail, s, h => execute rights tail (put s i none) h
  | .dead i :: tail, s, h => execute rights tail (put s i none) h
  | .address _ dst src :: tail, s, h => match s src with
    | some (.pointer p) => execute rights tail (put s dst (some (.pointer p))) h
    | _ => .fault .invalidLocal h
  | .read dst src :: tail, s, h => match s src with
    | some (.pointer p) => if rights.read p then match h p.address with
      | some v => execute rights tail (put s dst (some (.scalar v))) h
      | none => .fault .uninitialized h
      else .fault .deniedRead h
    | _ => .fault .invalidLocal h
  | .write dst src :: tail, s, h => match s dst, s src with
    | some (.pointer p), some (.scalar v) => if rights.write p then match h p.address with
      | some _ => execute rights tail (put s src none) (writeHeap h p.address v)
      | none => .fault .uninitialized h
      else .fault .deniedWrite h
    | _, _ => .fault .invalidLocal h
  | .move dst src :: tail, s, h => match s src with
    | some v => execute rights tail (put (put s src none) dst (some v)) h
    | none => .fault .invalidLocal h
  | .ret :: _, s, h => match s 0 with
    | some (.scalar v) => .returned v h
    | _ => .fault .invalidLocal h

end StdMemReplaceState
