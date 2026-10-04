import StackGenerated
open Aeneas Aeneas.Std Aeneas.Std.alloc.vec
namespace StackCorrespondence
abbrev Stack := RevmStack.revm_interpreter.interpreter.stack.Stack
abbrev Word := RevmStack.ruint.Uint 256#usize 4#usize

theorem length_all_inputs (stack : Stack) :
    ∃ n, RevmStack.revm_interpreter.interpreter.stack.Stack.len stack = .ok n ∧
      n.val = stack.data.val.length := by
  refine ⟨Vec.len stack.data, rfl, ?_⟩
  simp

theorem empty_all_inputs (stack : Stack) :
    RevmStack.revm_interpreter.interpreter.stack.Stack.is_empty stack =
      .ok (decide (stack.data.val.length = 0)) := by
  simp [RevmStack.revm_interpreter.interpreter.stack.Stack.is_empty,
    RevmStack.alloc.vec.Vec.is_empty, UScalar.eq_equiv]

theorem peek_valid (stack : Stack) (index : Usize)
    (bound : index.val < stack.data.val.length) :
    ∃ word, RevmStack.revm_interpreter.interpreter.stack.Stack.peek stack index =
      .ok (.Ok word) ∧
      word = stack.data.val[stack.data.val.length - index.val - 1]'(by omega) := by
  have hs : index.val ≤ (Vec.len stack.data).val := by simp; omega
  obtain ⟨diff, hd, hvd⟩ := WP.spec_imp_exists (UScalar.sub_spec hs)
  have hone : (1#usize).val ≤ diff.val := by simp at hvd ⊢; omega
  obtain ⟨i, hi, hvi⟩ := WP.spec_imp_exists (UScalar.sub_spec hone)
  have hiv : i.val = stack.data.val.length - index.val - 1 := by simp at hvd hvi; omega
  have hb : i.val < stack.data.length := by simpa [hiv] using (by omega : stack.data.val.length - index.val - 1 < stack.data.val.length)
  obtain ⟨word, hw, hword⟩ := WP.spec_imp_exists (Vec.index_usize_spec stack.data i hb)
  refine ⟨word, ?_, ?_⟩
  · simp [RevmStack.revm_interpreter.interpreter.stack.Stack.peek,
      UScalar.lt_equiv, bound, hd, hi, hw]
  · simpa only [hiv] using hword

theorem peek_underflow (stack : Stack) (index : Usize)
    (bound : stack.data.val.length ≤ index.val) :
    RevmStack.revm_interpreter.interpreter.stack.Stack.peek stack index =
      .ok (.Err .StackUnderflow) := by
  have h : ¬ index.val < stack.data.val.length := by omega
  simp [RevmStack.revm_interpreter.interpreter.stack.Stack.peek, UScalar.lt_equiv, h]

theorem peek_all_inputs (stack : Stack) (index : Usize) :
    RevmStack.revm_interpreter.interpreter.stack.Stack.peek stack index =
      .ok (if h : index.val < stack.data.val.length then
        .Ok (stack.data.val[stack.data.val.length - index.val - 1]'(by omega))
        else .Err .StackUnderflow) := by
  by_cases h : index.val < stack.data.val.length
  · obtain ⟨word, hp, hv⟩ := peek_valid stack index h
    simpa [h, hv] using hp
  · simpa [h] using peek_underflow stack index (by omega)

theorem peek_success_iff (stack : Stack) (index : Usize) :
    (∃ word, RevmStack.revm_interpreter.interpreter.stack.Stack.peek stack index =
      .ok (.Ok word)) ↔ index.val < stack.data.val.length := by
  constructor
  · intro success
    by_contra h
    obtain ⟨word, hp⟩ := success
    rw [peek_underflow stack index (by omega)] at hp
    simp at hp
  · intro h
    obtain ⟨word, hp, _⟩ := peek_valid stack index h
    exact ⟨word, hp⟩

end StackCorrespondence
