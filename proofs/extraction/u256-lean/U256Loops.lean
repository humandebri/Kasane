import U256Correspondence
open Aeneas Aeneas.Std
namespace U256Loops
abbrev Word := U256Extracted.ruint.Uint 256#usize 4#usize

theorem add_body_cont (self rhs : Word) (carry : Bool) (iter : Usize)
    (bound : iter.val < 4) :
    ∃ next : Word × Bool × Usize,
      U256Extracted.ruint.add.Uint.overflowing_add_loop.body rhs self carry iter =
        .ok (.cont next) ∧ next.2.2.val = iter.val + 1 := by
  have hs : iter.val < self.limbs.length := by simpa using bound
  have hr : iter.val < rhs.limbs.length := by simpa using bound
  obtain ⟨next, hn, hv, _⟩ := U256Correspondence.range_increment_defined iter bound
  obtain ⟨left, hl, _⟩ := WP.spec_imp_exists (Array.index_usize_spec self.limbs iter hs)
  obtain ⟨right, hr, _⟩ := WP.spec_imp_exists (Array.index_usize_spec rhs.limbs iter hr)
  obtain ⟨⟨lo, out⟩, hw, _⟩ := U256Correspondence.carrying_add_all_inputs left right carry
  obtain ⟨limbs, hu, _⟩ := WP.spec_imp_exists (Array.update_spec self.limbs iter lo hs)
  refine ⟨(⟨limbs⟩, out, next), ?_, hv⟩
  simp [U256Extracted.ruint.add.Uint.overflowing_add_loop.body,
    UScalar.lt_equiv, bound, hn, hl, hr, hw, hu]

theorem sub_body_cont (self rhs : Word) (borrow : Bool) (iter : Usize)
    (bound : iter.val < 4) :
    ∃ next : Word × Bool × Usize,
      U256Extracted.ruint.add.Uint.overflowing_sub_loop.body rhs self borrow iter =
        .ok (.cont next) ∧ next.2.2.val = iter.val + 1 := by
  have hs : iter.val < self.limbs.length := by simpa using bound
  have hr : iter.val < rhs.limbs.length := by simpa using bound
  obtain ⟨next, hn, hv, _⟩ := U256Correspondence.range_increment_defined iter bound
  obtain ⟨left, hl, _⟩ := WP.spec_imp_exists (Array.index_usize_spec self.limbs iter hs)
  obtain ⟨right, hr, _⟩ := WP.spec_imp_exists (Array.index_usize_spec rhs.limbs iter hr)
  obtain ⟨⟨lo, out⟩, hw, _⟩ := U256Correspondence.borrowing_sub_all_inputs left right borrow
  obtain ⟨limbs, hu, _⟩ := WP.spec_imp_exists (Array.update_spec self.limbs iter lo hs)
  refine ⟨(⟨limbs⟩, out, next), ?_, hv⟩
  simp [U256Extracted.ruint.add.Uint.overflowing_sub_loop.body,
    UScalar.lt_equiv, bound, hn, hl, hr, hw, hu]

theorem add_loop_total (fuel : Nat) (self rhs : Word) (carry : Bool) (iter : Usize)
    (remaining : iter.val + fuel = 4) :
    ∃ result, U256Extracted.ruint.add.Uint.overflowing_add_loop self rhs carry iter = .ok result := by
  induction fuel generalizing self carry iter with
  | zero =>
    have hi : iter.val = 4 := by omega
    refine ⟨(self, carry), ?_⟩
    rw [U256Extracted.ruint.add.Uint.overflowing_add_loop, loop.eq_1]
    simp [U256Extracted.ruint.add.Uint.overflowing_add_loop.body, UScalar.lt_equiv, hi]
  | succ fuel ih =>
    have hb : iter.val < 4 := by omega
    obtain ⟨⟨self1, carry1, iter1⟩, hs, hv⟩ := add_body_cont self rhs carry iter hb
    change iter1.val = iter.val + 1 at hv
    obtain ⟨result, hr⟩ := ih self1 carry1 iter1 (by omega)
    refine ⟨result, ?_⟩
    rw [U256Extracted.ruint.add.Uint.overflowing_add_loop, loop.eq_1]
    simp only [hs]
    simp only [bind_ok]
    exact hr

theorem sub_loop_total (fuel : Nat) (self rhs : Word) (borrow : Bool) (iter : Usize)
    (remaining : iter.val + fuel = 4) :
    ∃ result, U256Extracted.ruint.add.Uint.overflowing_sub_loop self rhs borrow iter = .ok result := by
  induction fuel generalizing self borrow iter with
  | zero =>
    have hi : iter.val = 4 := by omega
    refine ⟨(self, borrow), ?_⟩
    rw [U256Extracted.ruint.add.Uint.overflowing_sub_loop, loop.eq_1]
    simp [U256Extracted.ruint.add.Uint.overflowing_sub_loop.body, UScalar.lt_equiv, hi]
  | succ fuel ih =>
    have hb : iter.val < 4 := by omega
    obtain ⟨⟨self1, borrow1, iter1⟩, hs, hv⟩ := sub_body_cont self rhs borrow iter hb
    change iter1.val = iter.val + 1 at hv
    obtain ⟨result, hr⟩ := ih self1 borrow1 iter1 (by omega)
    refine ⟨result, ?_⟩
    rw [U256Extracted.ruint.add.Uint.overflowing_sub_loop, loop.eq_1]
    simp only [hs]
    simp only [bind_ok]
    exact hr

theorem overflowing_add_total (self rhs : Word) :
    ∃ result, U256Extracted.ruint.add.Uint.overflowing_add self rhs = .ok result := by
  obtain ⟨⟨out, carry⟩, hl⟩ := add_loop_total 4 self rhs false 0#usize (by simp)
  obtain ⟨last, hi, _⟩ := WP.spec_imp_exists
    (Array.index_usize_spec out.limbs 3#usize (by simp))
  have hs : (4#usize - 1#usize : Result Usize) = .ok 3#usize := by
    obtain ⟨out, ho, hv⟩ := WP.spec_imp_exists
      (@UScalar.sub_spec .Usize 4#usize 1#usize (by simp))
    have he : out = 3#usize := by
      apply UScalar.eq_of_val_eq
      simpa using hv
    simpa [he] using ho
  refine ⟨(out, carry || (last > core.num.U64.MAX)), ?_⟩
  simp [U256Extracted.ruint.add.Uint.overflowing_add, hl, hs, hi,
    U256Correspondence.mask256, U256Correspondence.masked256_identity]

theorem wrapping_add_total (self rhs : Word) :
    ∃ result, U256Extracted.ruint.add.Uint.wrapping_add self rhs = .ok result := by
  obtain ⟨⟨out, flag⟩, ho⟩ := overflowing_add_total self rhs
  refine ⟨out, ?_⟩
  simp [U256Extracted.ruint.add.Uint.wrapping_add, ho]

theorem overflowing_sub_total (self rhs : Word) :
    ∃ result, U256Extracted.ruint.add.Uint.overflowing_sub self rhs = .ok result := by
  obtain ⟨⟨out, borrow⟩, hl⟩ := sub_loop_total 4 self rhs false 0#usize (by simp)
  obtain ⟨last, hi, _⟩ := WP.spec_imp_exists
    (Array.index_usize_spec out.limbs 3#usize (by simp))
  have hs : (4#usize - 1#usize : Result Usize) = .ok 3#usize := by
    obtain ⟨out, ho, hv⟩ := WP.spec_imp_exists
      (@UScalar.sub_spec .Usize 4#usize 1#usize (by simp))
    have he : out = 3#usize := by
      apply UScalar.eq_of_val_eq
      simpa using hv
    simpa [he] using ho
  refine ⟨(out, borrow || (last > core.num.U64.MAX)), ?_⟩
  simp [U256Extracted.ruint.add.Uint.overflowing_sub, hl, hs, hi,
    U256Correspondence.mask256, U256Correspondence.masked256_identity]

theorem wrapping_sub_total (self rhs : Word) :
    ∃ result, U256Extracted.ruint.add.Uint.wrapping_sub self rhs = .ok result := by
  obtain ⟨⟨out, flag⟩, ho⟩ := overflowing_sub_total self rhs
  refine ⟨out, ?_⟩
  simp [U256Extracted.ruint.add.Uint.wrapping_sub, ho]

theorem add_total (a b : Word) : ∃ result, U256Extracted.add a b = .ok result := by
  exact wrapping_add_total a b

theorem sub_total (a b : Word) : ∃ result, U256Extracted.sub a b = .ok result := by
  exact wrapping_sub_total a b

end U256Loops
