import ReleaseTimestampSequence
namespace ReleaseTimestampLocalLifetime
inductive Event where
  | live (slot : Nat)
  | dead (slot : Nat)
  | copy (src dst : Nat)
  | borrow (src dst : Nat)
  | move (src dst : Nat)
  | call (args : List Nat) (dst : Nat)
  | ret (src : Nat)
  deriving DecidableEq
structure State where
  alive : List Nat
  initialized : List Nat
  deriving DecidableEq

def erase (i : Nat) (xs : List Nat) := xs.filter (fun j => j != i)
def ready (s : State) (i : Nat) : Bool := s.alive.contains i && s.initialized.contains i

def write (s : State) (dst : Nat) (consumed : List Nat) : State :=
  ⟨s.alive, dst :: s.initialized.filter (fun i => !consumed.contains i && i != dst)⟩

def step (consumeMoves : Bool) : Event → State → Option State
  | .live i, s => some ⟨i :: erase i s.alive, erase i s.initialized⟩
  | .dead i, s => some ⟨erase i s.alive, erase i s.initialized⟩
  | .copy src dst, s | .borrow src dst, s =>
    if ready s src && s.alive.contains dst then some (write s dst []) else none
  | .move src dst, s =>
    if ready s src && s.alive.contains dst then
      some (write s dst (if consumeMoves then [src] else [])) else none
  | .call args dst, s =>
    if args.all (ready s) && s.alive.contains dst then
      some (write s dst (if consumeMoves then args else [])) else none
  | .ret src, s => if ready s src then some s else none

def check (consumeMoves : Bool) : List Event → State → Option State
  | [], s => some s
  | event :: tail, s => (step consumeMoves event s).bind (check consumeMoves tail)

def initial : State := ⟨[0, 1, 2], [1, 2]⟩
def finalState : State := ⟨[0, 1, 2], [0, 1, 2]⟩

theorem check_append (policy : Bool) (xs ys : List Event) (s : State) :
    check policy (xs ++ ys) s = (check policy xs s).bind (check policy ys) := by
  induction xs generalizing s with
  | nil => rfl
  | cons event tail ih =>
    simp only [List.cons_append, check]
    cases h : step policy event s <;> simp [ih]

theorem checked_prefix (policy : Bool) (xs : List Event) (start finish : State)
    (valid : check policy xs start = some finish) (k : Nat) :
    ∃ intermediate, check policy (xs.take k) start = some intermediate := by
  have whole : check policy (xs.take k ++ xs.drop k) start = some finish := by
    simpa only [List.take_append_drop] using valid
  rw [check_append] at whole
  cases h : check policy (xs.take k) start with
  | none => simp [h] at whole
  | some intermediate => exact ⟨intermediate, rfl⟩

theorem dead_read_rejected (s : State) (i : Nat) :
    step true (.ret i) ⟨erase i s.alive, erase i s.initialized⟩ = none := by
  simp [step, ready, erase]

theorem live_resets_initialization (s : State) (i : Nat) :
    step true (.live i) s = some ⟨i :: erase i s.alive, erase i s.initialized⟩ := rfl

theorem repeated_dead (s : State) (i : Nat) :
    step true (.dead i) ⟨erase i s.alive, erase i s.initialized⟩ =
      step true (.dead i) s := by
  simp [step, erase, List.filter_filter]
end ReleaseTimestampLocalLifetime
