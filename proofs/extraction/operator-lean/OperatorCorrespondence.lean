import OperatorsGenerated
open Aeneas Aeneas.Std
namespace OperatorCorrespondence

theorem u64_and_all_inputs (a b : U64) :
    BitwiseOperators.U64.Insts.CoreOpsBitBitAndAssignU64.bitand_assign a b = .ok (a &&& b) := by rfl

theorem u64_or_all_inputs (a b : U64) :
    BitwiseOperators.U64.Insts.CoreOpsBitBitOrAssignU64.bitor_assign a b = .ok (a ||| b) := by rfl

theorem u64_xor_all_inputs (a b : U64) :
    BitwiseOperators.U64.Insts.CoreOpsBitBitXorAssignU64.bitxor_assign a b = .ok (a ^^^ b) := by rfl

abbrev Word := BitwiseOperators.ruint.Uint 256#usize 4#usize
abbrev Range := core.ops.range.Range Usize

def limb (w : Word) (j : Fin 4) : U64 := w.limbs.val[j.val]'(by simp)

theorem range_next (iter : Usize) (bound : iter.val < 4) :
    ∃ next : Usize,
      core.iter.range.IteratorRange.next core.iter.range.StepUsize ⟨iter, 4#usize⟩ =
        .ok (some iter, ⟨next, 4#usize⟩) ∧ next.val = iter.val + 1 := by
  have hm : 4 ≤ UScalar.max .Usize := by scalar_tac
  have fits : iter.val < UScalar.max .Usize := by omega
  let next := Usize.ofNatCore (iter.val + 1) (by scalar_tac)
  refine ⟨next, ?_, ?_⟩
  · simp [core.iter.range.IteratorRange.next, core.iter.range.StepUsize,
      core.iter.range.UScalarStep, core.cmp.PartialOrdUsize,
      core.cmp.impls.PartialOrdUsize.lt, core.clone.CloneUsize,
      core.iter.range.UScalarStep.forward_checked, bound, fits, next, liftFun1, liftFun2]
    apply UScalar.eq_of_val_eq
    simp
  · simp [next]

theorem range_done (iter : Usize) (bound : iter.val = 4) :
    core.iter.range.IteratorRange.next core.iter.range.StepUsize ⟨iter, 4#usize⟩ =
      .ok (none, ⟨iter, 4#usize⟩) := by
  simp [core.iter.range.IteratorRange.next, core.iter.range.StepUsize,
    core.iter.range.UScalarStep, core.cmp.PartialOrdUsize,
    core.cmp.impls.PartialOrdUsize.lt, bound, liftFun2]

theorem and_body (self rhs : Word) (iter : Usize) (bound : iter.val < 4) :
    ∃ next : Usize × Word,
      BitwiseOperators.ruint.Uint.Insts.CoreOpsBitBitAndAssignShared0Uint.bitand_assign_loop.body rhs ⟨iter, 4#usize⟩ self =
        .ok (.cont (⟨next.1, 4#usize⟩, next.2)) ∧
      next.1.val = iter.val + 1 ∧
      ∀ j : Fin 4, limb next.2 j =
        if j.val = iter.val then limb self j &&& limb rhs j else limb self j := by
  have hs : iter.val < self.limbs.length := by simpa using bound
  have hr : iter.val < rhs.limbs.length := by simpa using bound
  obtain ⟨next, hn, hv⟩ := range_next iter bound
  obtain ⟨⟨left, back⟩, hl, hleft, hback⟩ := WP.spec_imp_exists (Array.index_mut_usize_spec self.limbs iter hs)
  obtain ⟨right, hr, hright⟩ := WP.spec_imp_exists (Array.index_usize_spec rhs.limbs iter hr)
  refine ⟨(next, ⟨back (left &&& right)⟩), ?_, hv, ?_⟩
  · simp [BitwiseOperators.ruint.Uint.Insts.CoreOpsBitBitAndAssignShared0Uint.bitand_assign_loop.body, hn, hl, hr,
      BitwiseOperators.U64.Insts.CoreOpsBitBitAndAssignU64.bitand_assign]
  · intro j
    have hvals : (back (left &&& right)).val = self.limbs.val.set iter.val (left &&& right) := by simp [hback]
    unfold limb
    simp only [hvals]
    by_cases he : j.val = iter.val
    · simp [he, hleft, hright]
    · simp [he, Ne.symm he]

theorem or_body (self rhs : Word) (iter : Usize) (bound : iter.val < 4) :
    ∃ next : Usize × Word,
      BitwiseOperators.ruint.Uint.Insts.CoreOpsBitBitOrAssignShared0Uint.bitor_assign_loop.body rhs ⟨iter, 4#usize⟩ self =
        .ok (.cont (⟨next.1, 4#usize⟩, next.2)) ∧
      next.1.val = iter.val + 1 ∧
      ∀ j : Fin 4, limb next.2 j =
        if j.val = iter.val then limb self j ||| limb rhs j else limb self j := by
  have hs : iter.val < self.limbs.length := by simpa using bound
  have hr : iter.val < rhs.limbs.length := by simpa using bound
  obtain ⟨next, hn, hv⟩ := range_next iter bound
  obtain ⟨⟨left, back⟩, hl, hleft, hback⟩ := WP.spec_imp_exists (Array.index_mut_usize_spec self.limbs iter hs)
  obtain ⟨right, hr, hright⟩ := WP.spec_imp_exists (Array.index_usize_spec rhs.limbs iter hr)
  refine ⟨(next, ⟨back (left ||| right)⟩), ?_, hv, ?_⟩
  · simp [BitwiseOperators.ruint.Uint.Insts.CoreOpsBitBitOrAssignShared0Uint.bitor_assign_loop.body, hn, hl, hr,
      BitwiseOperators.U64.Insts.CoreOpsBitBitOrAssignU64.bitor_assign]
  · intro j
    have hvals : (back (left ||| right)).val = self.limbs.val.set iter.val (left ||| right) := by simp [hback]
    unfold limb
    simp only [hvals]
    by_cases he : j.val = iter.val
    · simp [he, hleft, hright]
    · simp [he, Ne.symm he]

theorem xor_body (self rhs : Word) (iter : Usize) (bound : iter.val < 4) :
    ∃ next : Usize × Word,
      BitwiseOperators.ruint.Uint.Insts.CoreOpsBitBitXorAssignShared0Uint.bitxor_assign_loop.body rhs ⟨iter, 4#usize⟩ self =
        .ok (.cont (⟨next.1, 4#usize⟩, next.2)) ∧
      next.1.val = iter.val + 1 ∧
      ∀ j : Fin 4, limb next.2 j =
        if j.val = iter.val then limb self j ^^^ limb rhs j else limb self j := by
  have hs : iter.val < self.limbs.length := by simpa using bound
  have hr : iter.val < rhs.limbs.length := by simpa using bound
  obtain ⟨next, hn, hv⟩ := range_next iter bound
  obtain ⟨⟨left, back⟩, hl, hleft, hback⟩ := WP.spec_imp_exists (Array.index_mut_usize_spec self.limbs iter hs)
  obtain ⟨right, hr, hright⟩ := WP.spec_imp_exists (Array.index_usize_spec rhs.limbs iter hr)
  refine ⟨(next, ⟨back (left ^^^ right)⟩), ?_, hv, ?_⟩
  · simp [BitwiseOperators.ruint.Uint.Insts.CoreOpsBitBitXorAssignShared0Uint.bitxor_assign_loop.body, hn, hl, hr,
      BitwiseOperators.U64.Insts.CoreOpsBitBitXorAssignU64.bitxor_assign]
  · intro j
    have hvals : (back (left ^^^ right)).val = self.limbs.val.set iter.val (left ^^^ right) := by simp [hback]
    unfold limb
    simp only [hvals]
    by_cases he : j.val = iter.val
    · simp [he, hleft, hright]
    · simp [he, Ne.symm he]

theorem loop_spec (op : U64 → U64 → U64)
    (body : Range → Word → Result (ControlFlow (Range × Word) Word)) (rhs : Word)
    (step : ∀ self iter, iter.val < 4 → ∃ next : Usize × Word,
      body ⟨iter, 4#usize⟩ self = .ok (.cont (⟨next.1, 4#usize⟩, next.2)) ∧
      next.1.val = iter.val + 1 ∧ ∀ j : Fin 4, limb next.2 j =
        if j.val = iter.val then op (limb self j) (limb rhs j) else limb self j)
    (stop : ∀ self iter, iter.val = 4 → body ⟨iter, 4#usize⟩ self = .ok (.done self))
    (fuel : Nat) (self : Word) (iter : Usize) (remaining : iter.val + fuel = 4) :
    ∃ out, loop (fun (r, w) => body r w) (⟨iter, 4#usize⟩, self) = .ok out ∧
      ∀ j : Fin 4, limb out j =
        if iter.val ≤ j.val then op (limb self j) (limb rhs j) else limb self j := by
  induction fuel generalizing self iter with
  | zero =>
    have hi : iter.val = 4 := by omega
    refine ⟨self, ?_, ?_⟩
    · rw [loop.eq_1]
      dsimp only
      rw [stop self iter hi]
      simp
    · intro j
      have hj := j.isLt
      simp [if_neg (by omega : ¬ iter.val ≤ j.val)]
  | succ fuel ih =>
    obtain ⟨⟨i, w⟩, hb, hv, hj⟩ := step self iter (by omega)
    change i.val = iter.val + 1 at hv
    obtain ⟨out, ho, hout⟩ := ih w i (by omega)
    refine ⟨out, ?_, ?_⟩
    · rw [loop.eq_1]
      dsimp only
      rw [hb]
      simp only [bind_ok]
      exact ho
    · intro j
      rw [hout j, hj j]
      split_ifs <;> simp_all <;> omega

theorem operator_and_all_inputs (self rhs : Word) :
    ∃ out, BitwiseOperators.operator_and self rhs = .ok out ∧
      ∀ j : Fin 4, limb out j = limb self j &&& limb rhs j := by
  obtain ⟨out, ho, hv⟩ := loop_spec (fun a b => a &&& b)
    (BitwiseOperators.ruint.Uint.Insts.CoreOpsBitBitAndAssignShared0Uint.bitand_assign_loop.body rhs) rhs
    (fun w i h => and_body w rhs i h)
    (by intro w i h; simp [BitwiseOperators.ruint.Uint.Insts.CoreOpsBitBitAndAssignShared0Uint.bitand_assign_loop.body, range_done i h])
    4 self 0#usize (by simp)
  refine ⟨out, ?_, ?_⟩
  · exact ho
  · intro j
    simpa using hv j

theorem operator_or_all_inputs (self rhs : Word) :
    ∃ out, BitwiseOperators.operator_or self rhs = .ok out ∧
      ∀ j : Fin 4, limb out j = limb self j ||| limb rhs j := by
  obtain ⟨out, ho, hv⟩ := loop_spec (fun a b => a ||| b)
    (BitwiseOperators.ruint.Uint.Insts.CoreOpsBitBitOrAssignShared0Uint.bitor_assign_loop.body rhs) rhs
    (fun w i h => or_body w rhs i h)
    (by intro w i h; simp [BitwiseOperators.ruint.Uint.Insts.CoreOpsBitBitOrAssignShared0Uint.bitor_assign_loop.body, range_done i h])
    4 self 0#usize (by simp)
  refine ⟨out, ?_, ?_⟩
  · exact ho
  · intro j
    simpa using hv j

theorem operator_xor_all_inputs (self rhs : Word) :
    ∃ out, BitwiseOperators.operator_xor self rhs = .ok out ∧
      ∀ j : Fin 4, limb out j = limb self j ^^^ limb rhs j := by
  obtain ⟨out, ho, hv⟩ := loop_spec (fun a b => a ^^^ b)
    (BitwiseOperators.ruint.Uint.Insts.CoreOpsBitBitXorAssignShared0Uint.bitxor_assign_loop.body rhs) rhs
    (fun w i h => xor_body w rhs i h)
    (by intro w i h; simp [BitwiseOperators.ruint.Uint.Insts.CoreOpsBitBitXorAssignShared0Uint.bitxor_assign_loop.body, range_done i h])
    4 self 0#usize (by simp)
  refine ⟨out, ?_, ?_⟩
  · exact ho
  · intro j
    simpa using hv j

def bit (w : Word) (k : Fin 256) : Bool :=
  (limb w ⟨k.val / 64, by have h := k.isLt; omega⟩).bv.getLsbD (k.val % 64)

theorem operator_and_all_bits (self rhs : Word) :
    ∃ out, BitwiseOperators.operator_and self rhs = .ok out ∧
      ∀ k : Fin 256, bit out k = (bit self k && bit rhs k) := by
  obtain ⟨out, ho, hv⟩ := operator_and_all_inputs self rhs
  refine ⟨out, ho, ?_⟩
  intro k
  have h := congrArg (fun x : U64 => x.bv.getLsbD (k.val % 64))
    (hv ⟨k.val / 64, by have hk := k.isLt; omega⟩)
  simpa [bit] using h

theorem operator_or_all_bits (self rhs : Word) :
    ∃ out, BitwiseOperators.operator_or self rhs = .ok out ∧
      ∀ k : Fin 256, bit out k = (bit self k || bit rhs k) := by
  obtain ⟨out, ho, hv⟩ := operator_or_all_inputs self rhs
  refine ⟨out, ho, ?_⟩
  intro k
  have h := congrArg (fun x : U64 => x.bv.getLsbD (k.val % 64))
    (hv ⟨k.val / 64, by have hk := k.isLt; omega⟩)
  simpa [bit] using h

theorem operator_xor_all_bits (self rhs : Word) :
    ∃ out, BitwiseOperators.operator_xor self rhs = .ok out ∧
      ∀ k : Fin 256, bit out k = (bit self k ^^ bit rhs k) := by
  obtain ⟨out, ho, hv⟩ := operator_xor_all_inputs self rhs
  refine ⟨out, ho, ?_⟩
  intro k
  have h := congrArg (fun x : U64 => x.bv.getLsbD (k.val % 64))
    (hv ⟨k.val / 64, by have hk := k.isLt; omega⟩)
  simpa [bit] using h

end OperatorCorrespondence
