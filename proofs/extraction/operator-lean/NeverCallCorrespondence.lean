import LedgerActualUnwrap.Funs
open Aeneas Aeneas.Std
namespace NeverCallCorrespondence

def eliminate {T : Type} (n : Never) : Result T := nomatch n

theorem continuations_equal {T : Type} (r : Result Never)
    (f g : Never → Result T) : Aeneas.Std.bind r f = Aeneas.Std.bind r g := by
  have h : f = g := funext (fun n => nomatch n)
  rw [h]

theorem cannot_return (r : Result Never) : ¬ ∃ n, r = .ok n := by
  rintro ⟨n, _⟩
  nomatch n

theorem preserves_failure {T : Type} (e : Error) :
    Aeneas.Std.bind (.fail e : Result Never) (@eliminate T) = .fail e := by simp

theorem preserves_divergence {T : Type} :
    Aeneas.Std.bind (Result.div : Result Never) (@eliminate T) = .div := by simp

theorem unwrap_ok_all_inputs {T E : Type} (inst : LedgerActualUnwrap.core.fmt.Debug E)
    (t : T) : LedgerActualUnwrap.core.result.Result.unwrap inst (.Ok t) = .ok t := by rfl

theorem unwrap_err_observation {T E : Type} (inst : LedgerActualUnwrap.core.fmt.Debug E)
    (e : E) : LedgerActualUnwrap.core.result.Result.unwrap (T := T) inst (.Err e) =
      .fail .panic := by
  simp [LedgerActualUnwrap.core.result.Result.unwrap, LedgerActualUnwrap.core.result.unwrap_failed]
end NeverCallCorrespondence
