import BitwiseGenerated
import U256Correspondence
open Aeneas Aeneas.Std
namespace BitwiseCorrespondence
abbrev Word := BitwiseExtracted.ruint.Uint 256#usize 4#usize

def limb (w : Word) (j : Fin 4) : U64 := w.limbs.val[j.val]'(by simp)

theorem unchecked_increment (iter : Usize) (bound : iter.val < 4) :
    ∃ next, BitwiseExtracted.core.num.Usize.unchecked_add iter 1#usize = .ok next ∧
      next.val = iter.val + 1 := by
  obtain ⟨next, hn, hv, _⟩ := U256Correspondence.range_increment_defined iter bound
  refine ⟨next, ?_, hv⟩
  simpa [U256Extracted.core.num.Usize.unchecked_add,
    U256Extracted.core.ub_checks.check_language_ub,
    U256Extracted.core.ub_checks.check_language_ub.runtime,
    BitwiseExtracted.core.num.Usize.unchecked_add,
    BitwiseExtracted.core.ub_checks.check_language_ub,
    BitwiseExtracted.core.ub_checks.check_language_ub.runtime] using hn

theorem bitand_body (self rhs : Word) (iter : Usize) (bound : iter.val < 4) :
    ∃ next : Word × Usize,
      BitwiseExtracted.ruint.bits.Uint.bitand_loop.body rhs self iter = .ok (.cont next) ∧
      next.2.val = iter.val + 1 ∧
      ∀ j : Fin 4, limb next.1 j =
        if j.val = iter.val then limb self j &&& limb rhs j else limb self j := by
  have hs : iter.val < self.limbs.length := by simpa using bound
  have hr : iter.val < rhs.limbs.length := by simpa using bound
  obtain ⟨next, hn, hv⟩ := unchecked_increment iter bound
  obtain ⟨left, hl, hleft⟩ := WP.spec_imp_exists (Array.index_usize_spec self.limbs iter hs)
  obtain ⟨right, hr, hright⟩ := WP.spec_imp_exists (Array.index_usize_spec rhs.limbs iter hr)
  obtain ⟨limbs, hu, hlimbs⟩ := WP.spec_imp_exists (Array.update_spec self.limbs iter (left &&& right) hs)
  refine ⟨(⟨limbs⟩, next), ?_, hv, ?_⟩
  · simp [BitwiseExtracted.ruint.bits.Uint.bitand_loop.body,
      UScalar.lt_equiv, bound, hn, hl, hr, hu, lift]
  · intro j
    have hvals : limbs.val = self.limbs.val.set iter.val (left &&& right) := by simp [hlimbs]
    unfold limb
    simp only [hvals]
    by_cases he : j.val = iter.val
    · simp [he, hleft, hright]
    · simp [he, Ne.symm he]

theorem bitor_body (self rhs : Word) (iter : Usize) (bound : iter.val < 4) :
    ∃ next : Word × Usize,
      BitwiseExtracted.ruint.bits.Uint.bitor_loop.body rhs self iter = .ok (.cont next) ∧
      next.2.val = iter.val + 1 ∧
      ∀ j : Fin 4, limb next.1 j =
        if j.val = iter.val then limb self j ||| limb rhs j else limb self j := by
  have hs : iter.val < self.limbs.length := by simpa using bound
  have hr : iter.val < rhs.limbs.length := by simpa using bound
  obtain ⟨next, hn, hv⟩ := unchecked_increment iter bound
  obtain ⟨left, hl, hleft⟩ := WP.spec_imp_exists (Array.index_usize_spec self.limbs iter hs)
  obtain ⟨right, hr, hright⟩ := WP.spec_imp_exists (Array.index_usize_spec rhs.limbs iter hr)
  obtain ⟨limbs, hu, hlimbs⟩ := WP.spec_imp_exists (Array.update_spec self.limbs iter (left ||| right) hs)
  refine ⟨(⟨limbs⟩, next), ?_, hv, ?_⟩
  · simp [BitwiseExtracted.ruint.bits.Uint.bitor_loop.body,
      UScalar.lt_equiv, bound, hn, hl, hr, hu, lift]
  · intro j
    have hvals : limbs.val = self.limbs.val.set iter.val (left ||| right) := by simp [hlimbs]
    unfold limb
    simp only [hvals]
    by_cases he : j.val = iter.val
    · simp [he, hleft, hright]
    · simp [he, Ne.symm he]

theorem bitxor_body (self rhs : Word) (iter : Usize) (bound : iter.val < 4) :
    ∃ next : Word × Usize,
      BitwiseExtracted.ruint.bits.Uint.bitxor_loop.body rhs self iter = .ok (.cont next) ∧
      next.2.val = iter.val + 1 ∧
      ∀ j : Fin 4, limb next.1 j =
        if j.val = iter.val then limb self j ^^^ limb rhs j else limb self j := by
  have hs : iter.val < self.limbs.length := by simpa using bound
  have hr : iter.val < rhs.limbs.length := by simpa using bound
  obtain ⟨next, hn, hv⟩ := unchecked_increment iter bound
  obtain ⟨left, hl, hleft⟩ := WP.spec_imp_exists (Array.index_usize_spec self.limbs iter hs)
  obtain ⟨right, hr, hright⟩ := WP.spec_imp_exists (Array.index_usize_spec rhs.limbs iter hr)
  obtain ⟨limbs, hu, hlimbs⟩ := WP.spec_imp_exists (Array.update_spec self.limbs iter (left ^^^ right) hs)
  refine ⟨(⟨limbs⟩, next), ?_, hv, ?_⟩
  · simp [BitwiseExtracted.ruint.bits.Uint.bitxor_loop.body,
      UScalar.lt_equiv, bound, hn, hl, hr, hu, lift]
  · intro j
    have hvals : limbs.val = self.limbs.val.set iter.val (left ^^^ right) := by simp [hlimbs]
    unfold limb
    simp only [hvals]
    by_cases he : j.val = iter.val
    · simp [he, hleft, hright]
    · simp [he, Ne.symm he]

theorem loop_spec (op : U64 → U64 → U64)
    (body : Word → Usize → Result (ControlFlow (Word × Usize) Word)) (rhs : Word)
    (step : ∀ self iter, iter.val < 4 → ∃ next : Word × Usize,
      body self iter = .ok (.cont next) ∧ next.2.val = iter.val + 1 ∧
      ∀ j : Fin 4, limb next.1 j =
        if j.val = iter.val then op (limb self j) (limb rhs j) else limb self j)
    (stop : ∀ self iter, iter.val = 4 → body self iter = .ok (.done self))
    (fuel : Nat) (self : Word) (iter : Usize) (remaining : iter.val + fuel = 4) :
    ∃ out, loop (fun (w, i) => body w i) (self, iter) = .ok out ∧
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
    obtain ⟨⟨w, i⟩, hb, hv, hj⟩ := step self iter (by omega)
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

theorem bitand_all_inputs (self rhs : Word) :
    ∃ out, BitwiseExtracted.bitand self rhs = .ok out ∧
      ∀ j : Fin 4, limb out j = limb self j &&& limb rhs j := by
  obtain ⟨out, ho, hv⟩ := loop_spec (fun a b => a &&& b)
    (BitwiseExtracted.ruint.bits.Uint.bitand_loop.body rhs) rhs
    (fun w i h => bitand_body w rhs i h)
    (by intro w i h; simp [BitwiseExtracted.ruint.bits.Uint.bitand_loop.body, UScalar.lt_equiv, h])
    4 self 0#usize (by simp)
  refine ⟨out, ?_, ?_⟩
  · exact ho
  · intro j
    simpa using hv j

theorem bitor_all_inputs (self rhs : Word) :
    ∃ out, BitwiseExtracted.bitor self rhs = .ok out ∧
      ∀ j : Fin 4, limb out j = limb self j ||| limb rhs j := by
  obtain ⟨out, ho, hv⟩ := loop_spec (fun a b => a ||| b)
    (BitwiseExtracted.ruint.bits.Uint.bitor_loop.body rhs) rhs
    (fun w i h => bitor_body w rhs i h)
    (by intro w i h; simp [BitwiseExtracted.ruint.bits.Uint.bitor_loop.body, UScalar.lt_equiv, h])
    4 self 0#usize (by simp)
  refine ⟨out, ?_, ?_⟩
  · exact ho
  · intro j
    simpa using hv j

theorem bitxor_all_inputs (self rhs : Word) :
    ∃ out, BitwiseExtracted.bitxor self rhs = .ok out ∧
      ∀ j : Fin 4, limb out j = limb self j ^^^ limb rhs j := by
  obtain ⟨out, ho, hv⟩ := loop_spec (fun a b => a ^^^ b)
    (BitwiseExtracted.ruint.bits.Uint.bitxor_loop.body rhs) rhs
    (fun w i h => bitxor_body w rhs i h)
    (by intro w i h; simp [BitwiseExtracted.ruint.bits.Uint.bitxor_loop.body, UScalar.lt_equiv, h])
    4 self 0#usize (by simp)
  refine ⟨out, ?_, ?_⟩
  · exact ho
  · intro j
    simpa using hv j

def bit (w : Word) (k : Fin 256) : Bool :=
  (limb w ⟨k.val / 64, by have h := k.isLt; omega⟩).bv.getLsbD (k.val % 64)

theorem bitand_all_bits (self rhs : Word) :
    ∃ out, BitwiseExtracted.bitand self rhs = .ok out ∧
      ∀ k : Fin 256, bit out k = (bit self k && bit rhs k) := by
  obtain ⟨out, ho, hv⟩ := bitand_all_inputs self rhs
  refine ⟨out, ho, ?_⟩
  intro k
  have h := congrArg (fun x : U64 => x.bv.getLsbD (k.val % 64))
    (hv ⟨k.val / 64, by have hk := k.isLt; omega⟩)
  simpa [bit] using h

theorem bitor_all_bits (self rhs : Word) :
    ∃ out, BitwiseExtracted.bitor self rhs = .ok out ∧
      ∀ k : Fin 256, bit out k = (bit self k || bit rhs k) := by
  obtain ⟨out, ho, hv⟩ := bitor_all_inputs self rhs
  refine ⟨out, ho, ?_⟩
  intro k
  have h := congrArg (fun x : U64 => x.bv.getLsbD (k.val % 64))
    (hv ⟨k.val / 64, by have hk := k.isLt; omega⟩)
  simpa [bit] using h

theorem bitxor_all_bits (self rhs : Word) :
    ∃ out, BitwiseExtracted.bitxor self rhs = .ok out ∧
      ∀ k : Fin 256, bit out k = (bit self k ^^ bit rhs k) := by
  obtain ⟨out, ho, hv⟩ := bitxor_all_inputs self rhs
  refine ⟨out, ho, ?_⟩
  intro k
  have h := congrArg (fun x : U64 => x.bv.getLsbD (k.val % 64))
    (hv ⟨k.val / 64, by have hk := k.isLt; omega⟩)
  simpa [bit] using h

end BitwiseCorrespondence
