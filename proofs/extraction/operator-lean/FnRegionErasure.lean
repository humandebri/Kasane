import Mathlib.Tactic

namespace FnRegionErasure

-- Integer-depth model of Charon's shift_subst / erase callback.
-- No AST traversal, lifetime soundness, pointer ABI or interpreter refinement is asserted.
inductive Region where
  | free (id : Nat)
  | bound (depth : Int) (id : Nat)
  | erased
  | static
  deriving DecidableEq

def shift (delta : Int) (r : Region) : Region :=
  match r with
  | .bound d id => .bound (d + delta) id
  | _ => r

def eraseCallback (r : Region) : Region :=
  match r with
  | .bound d _ => if d < 0 then r else .erased
  | .free _ => .erased
  | _ => r

def throughBinders (n : Nat) (r : Region) : Region :=
  shift (n : Int) (eraseCallback (shift (-(n : Int)) r))

theorem free_erased (n id : Nat) : throughBinders n (.free id) = .erased := by rfl

theorem local_bound_preserved (n id : Nat) (d : Int)
    (hlocal : d < (n : Int)) : throughBinders n (.bound d id) = .bound d id := by
  have h : d + -(n : Int) < 0 := by omega
  simp [throughBinders, shift, eraseCallback, h]

theorem outer_bound_erased (n id : Nat) (d : Int)
    (houter : (n : Int) ≤ d) : throughBinders n (.bound d id) = .erased := by
  have h : ¬ d + -(n : Int) < 0 := by omega
  simp [throughBinders, shift, eraseCallback, h]

theorem erased_preserved (n : Nat) : throughBinders n .erased = .erased := by rfl

theorem shifts_compose (a b : Int) (r : Region) :
    shift a (shift b r) = shift (a + b) r := by
  cases r <;> simp [shift]
  omega

theorem erasure_idempotent (n : Nat) (r : Region) :
    throughBinders n (throughBinders n r) = throughBinders n r := by
  cases r with
  | free id => rfl
  | erased => rfl
  | static => rfl
  | bound d id =>
    by_cases h : d < (n : Int)
    · rw [local_bound_preserved n id d h, local_bound_preserved n id d h]
    · have houter : (n : Int) ≤ d := by omega
      rw [outer_bound_erased n id d houter]
      rfl

end FnRegionErasure
