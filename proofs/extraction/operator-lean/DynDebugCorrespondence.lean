import LedgerUnwrapFailed.Funs
open Aeneas Aeneas.Std
namespace DynDebugCorrespondence

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

theorem unwrap_ok_all_inputs {T E : Type} (inst : LedgerUnwrapFailed.core.fmt.Debug E)
    (t : T) : LedgerUnwrapFailed.core.result.Result.unwrap inst (.Ok t) = .ok t := by rfl

theorem unwrap_err_observation {T E : Type} (inst : LedgerUnwrapFailed.core.fmt.Debug E)
    (e : E) : LedgerUnwrapFailed.core.result.Result.unwrap (T := T) inst (.Err e) =
      .fail .panic := by
  simp [LedgerUnwrapFailed.core.result.Result.unwrap, LedgerUnwrapFailed.core.result.unwrap_failed, LedgerUnwrapFailed.core.fmt.rt.Argument.new_display, LedgerUnwrapFailed.core.fmt.rt.Argument.new_debug, LedgerUnwrapFailed.core.fmt.Arguments.new]


def dynamicDebug : LedgerUnwrapFailed.core.fmt.Debug (Dyn (fun E => LedgerUnwrapFailed.core.fmt.Debug E)) :=
  { fmt := fun d f => d.inst.fmt d.value f }

theorem dynamic_dispatch_all_inputs (d : Dyn (fun E => LedgerUnwrapFailed.core.fmt.Debug E))
    (f : LedgerUnwrapFailed.core.fmt.Formatter) : dynamicDebug.fmt d f = d.inst.fmt d.value f := by rfl

theorem dynamic_dispatch_failure (d : Dyn (fun E => LedgerUnwrapFailed.core.fmt.Debug E))
    (f : LedgerUnwrapFailed.core.fmt.Formatter) (e : Error) (h : d.inst.fmt d.value f = .fail e) :
    dynamicDebug.fmt d f = .fail e := by exact h

theorem dynamic_dispatch_divergence (d : Dyn (fun E => LedgerUnwrapFailed.core.fmt.Debug E))
    (f : LedgerUnwrapFailed.core.fmt.Formatter) (h : d.inst.fmt d.value f = .div) :
    dynamicDebug.fmt d f = .div := by exact h

theorem new_debug_captures_receiver {T : Type u} (inst : LedgerUnwrapFailed.core.fmt.Debug T) (x : T) :
    LedgerUnwrapFailed.core.fmt.rt.Argument.new_debug inst x = .ok ⟨inst.fmt x⟩ := by rfl

theorem new_display_captures_receiver {T : Type u} (inst : LedgerUnwrapFailed.core.fmt.Display T) (x : T) :
    LedgerUnwrapFailed.core.fmt.rt.Argument.new_display inst x = .ok ⟨inst.fmt x⟩ := by rfl

theorem new_debug_render {T : Type u} (inst : LedgerUnwrapFailed.core.fmt.Debug T) (x : T)
    (f : LedgerUnwrapFailed.core.fmt.Formatter) :
    (do let a ← LedgerUnwrapFailed.core.fmt.rt.Argument.new_debug inst x; a.render f) = inst.fmt x f := by
  simp [LedgerUnwrapFailed.core.fmt.rt.Argument.new_debug, LedgerUnwrapFailed.core.fmt.rt.Argument.render]

theorem new_display_render {T : Type u} (inst : LedgerUnwrapFailed.core.fmt.Display T) (x : T)
    (f : LedgerUnwrapFailed.core.fmt.Formatter) :
    (do let a ← LedgerUnwrapFailed.core.fmt.rt.Argument.new_display inst x; a.render f) = inst.fmt x f := by
  simp [LedgerUnwrapFailed.core.fmt.rt.Argument.new_display, LedgerUnwrapFailed.core.fmt.rt.Argument.render]

theorem arguments_retain_all_inputs {N M : Usize} (bytes : Std.Array U8 N)
    (args : Std.Array LedgerUnwrapFailed.core.fmt.rt.Argument M) :
    LedgerUnwrapFailed.core.fmt.Arguments.new bytes args = .ok ⟨bytes.val, args.val⟩ := by rfl

theorem source_unwrap_failed_observation (msg : Str)
    (error : Dyn (fun E => LedgerUnwrapFailed.core.fmt.Debug E)) :
    LedgerUnwrapFailed.core.result.unwrap_failed msg error = .fail .panic := by
  simp [LedgerUnwrapFailed.core.result.unwrap_failed, LedgerUnwrapFailed.core.fmt.rt.Argument.new_display,
    LedgerUnwrapFailed.core.fmt.rt.Argument.new_debug, LedgerUnwrapFailed.core.fmt.Arguments.new]
end DynDebugCorrespondence
