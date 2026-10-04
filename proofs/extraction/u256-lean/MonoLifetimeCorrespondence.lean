import MonoLifetimeGenerated
open Aeneas
namespace MonoLifetimeCorrespondence
open MonoLifetimeGeneric
-- Properties of the generated lifetime-only fixture, not EVM or Rust heap refinement.
theorem same_noop_exact (context : Same) : same_noop context = .ok context := rfl

theorem split_noop_exact (context : Split) :
    split_noop context = .ok (context, context) := rfl

theorem split_left_exact (context : Split) :
    split_left context = .ok (context.left, (fun value => { context with left := value }), context) := rfl

theorem split_right_exact (context : Split) :
    split_right context = .ok (context.right, context, (fun value => { context with right := value })) := rfl

theorem left_update_selected (context : Split) (value : Aeneas.Std.U32) :
    ((fun value => { context with left := value }) value).left = value := rfl

theorem left_update_preserves_other (context : Split) (value : Aeneas.Std.U32) :
    ((fun value => { context with left := value }) value).right = context.right := rfl

theorem right_update_selected (context : Split) (value : Aeneas.Std.U32) :
    ((fun value => { context with right := value }) value).right = value := rfl

theorem right_update_preserves_other (context : Split) (value : Aeneas.Std.U32) :
    ((fun value => { context with right := value }) value).left = context.left := rfl
end MonoLifetimeCorrespondence
