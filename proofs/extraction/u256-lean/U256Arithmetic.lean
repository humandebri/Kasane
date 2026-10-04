import U256Loops
import LimbComposition
open Aeneas Aeneas.Std
namespace U256Arithmetic
open WordCorrespondence
open U256Loops

theorem value_set_balance (xs : List U64) (i : Nat) (x : U64)
    (bound : i < xs.length) :
    LimbComposition.value (xs.set i x) + radix ^ i * xs[i].val =
      LimbComposition.value xs + radix ^ i * x.val := by
  induction xs generalizing i with
  | nil => simp at bound
  | cons a rest ih =>
    cases i with
    | zero => simp [LimbComposition.value, Nat.add_comm, Nat.add_assoc, Nat.add_left_comm]
    | succ i =>
      have hb : i < rest.length := by simpa using bound
      have h := ih i hb
      simp only [List.set, List.getElem_cons_succ, LimbComposition.value, pow_succ]
      nlinarith only [h]

theorem value_drop_step (xs : List U64) (i : Nat) (bound : i < xs.length) :
    LimbComposition.value (xs.drop i) =
      xs[i].val + radix * LimbComposition.value (xs.drop (i + 1)) := by
  induction xs generalizing i with
  | nil => simp at bound
  | cons a rest ih =>
    cases i with
    | zero => simp [LimbComposition.value]
    | succ i =>
      have hb : i < rest.length := by simpa using bound
      simpa using ih i hb

theorem add_body_balance (self rhs : Word) (carry : Bool) (iter : Usize)
    (bound : iter.val < 4) :
    ∃ next : Word × Bool × Usize,
      U256Extracted.ruint.add.Uint.overflowing_add_loop.body rhs self carry iter =
        .ok (.cont next) ∧ next.2.2.val = iter.val + 1 ∧
      LimbComposition.value next.1.limbs.val + radix ^ (iter.val + 1) * next.2.1.toNat =
        LimbComposition.value self.limbs.val +
          radix ^ iter.val * ((rhs.limbs.val[iter.val]'(by simpa using bound)).val + carry.toNat) := by
  have hs : iter.val < self.limbs.length := by simpa using bound
  have hr : iter.val < rhs.limbs.length := by simpa using bound
  obtain ⟨next, hn, hv, _⟩ := U256Correspondence.range_increment_defined iter bound
  obtain ⟨left, hl, hleft⟩ := WP.spec_imp_exists (Array.index_usize_spec self.limbs iter hs)
  obtain ⟨right, hr, hright⟩ := WP.spec_imp_exists (Array.index_usize_spec rhs.limbs iter hr)
  obtain ⟨⟨lo, out⟩, hw, hword⟩ := U256Correspondence.carrying_add_all_inputs left right carry
  obtain ⟨limbs, hu, hlimbs⟩ := WP.spec_imp_exists (Array.update_spec self.limbs iter lo hs)
  refine ⟨(⟨limbs⟩, out, next), ?_, hv, ?_⟩
  · simp [U256Extracted.ruint.add.Uint.overflowing_add_loop.body,
      UScalar.lt_equiv, bound, hn, hl, hr, hw, hu]
  · have hset := value_set_balance self.limbs.val iter.val lo hs
    have hflag : lo.val + radix * out.toNat = left.val + right.val + carry.toNat := by
      cases he : out <;> simpa [he] using hword
    have hvals : limbs.val = self.limbs.val.set iter.val lo := by
      simp [hlimbs]
    change LimbComposition.value limbs.val + radix ^ (iter.val + 1) * out.toNat = _
    rw [hvals]
    rw [hleft, hright] at hflag
    simp only [pow_succ]
    have scaled := congrArg (fun n : Nat => radix ^ iter.val * n) hflag
    nlinarith only [hset, scaled]

theorem add_loop_balance (fuel : Nat) (self rhs : Word) (carry : Bool) (iter : Usize)
    (remaining : iter.val + fuel = 4) :
    ∃ result : Word × Bool,
      U256Extracted.ruint.add.Uint.overflowing_add_loop self rhs carry iter = .ok result ∧
      LimbComposition.value result.1.limbs.val + radix ^ 4 * result.2.toNat =
        LimbComposition.value self.limbs.val +
          radix ^ iter.val * (LimbComposition.value (rhs.limbs.val.drop iter.val) + carry.toNat) := by
  induction fuel generalizing self carry iter with
  | zero =>
    have hi : iter.val = 4 := by omega
    refine ⟨(self, carry), ?_, ?_⟩
    · rw [U256Extracted.ruint.add.Uint.overflowing_add_loop, loop.eq_1]
      simp [U256Extracted.ruint.add.Uint.overflowing_add_loop.body, UScalar.lt_equiv, hi]
    · simp [hi, LimbComposition.value]
  | succ fuel ih =>
    have hb : iter.val < 4 := by omega
    obtain ⟨⟨self1, carry1, iter1⟩, hs, hv, hbal⟩ := add_body_balance self rhs carry iter hb
    change iter1.val = iter.val + 1 at hv
    change LimbComposition.value self1.limbs.val + radix ^ (iter.val + 1) * carry1.toNat = _ at hbal
    obtain ⟨result, hr, hresult⟩ := ih self1 carry1 iter1 (by omega)
    refine ⟨result, ?_, ?_⟩
    · rw [U256Extracted.ruint.add.Uint.overflowing_add_loop, loop.eq_1]
      simp only [hs, bind_ok]
      exact hr
    · have hdrop := value_drop_step rhs.limbs.val iter.val (by simpa using hb)
      rw [hv] at hresult
      rw [hdrop]
      simp only [pow_succ] at hbal hresult
      nlinarith only [hbal, hresult]

theorem wrapping_add_from_loop (self rhs out : Word) (carry : Bool)
    (hl : U256Extracted.ruint.add.Uint.overflowing_add_loop self rhs false 0#usize = .ok (out, carry)) :
    U256Extracted.ruint.add.Uint.wrapping_add self rhs = .ok out := by
  obtain ⟨last, hi, _⟩ := WP.spec_imp_exists
    (Array.index_usize_spec out.limbs 3#usize (by simp))
  have hs : (4#usize - 1#usize : Result Usize) = .ok 3#usize := by
    obtain ⟨out, ho, hv⟩ := WP.spec_imp_exists
      (@UScalar.sub_spec .Usize 4#usize 1#usize (by simp))
    have he : out = 3#usize := by
      apply UScalar.eq_of_val_eq
      simpa using hv
    simpa [he] using ho
  simp [U256Extracted.ruint.add.Uint.wrapping_add,
    U256Extracted.ruint.add.Uint.overflowing_add, hl, hs, hi,
    U256Correspondence.mask256, U256Correspondence.masked256_identity]

theorem wrapping_add_all_inputs (a b : Word) :
    ∃ out, U256Extracted.ruint.add.Uint.wrapping_add a b = .ok out ∧
      LimbComposition.value out.limbs.val =
        (LimbComposition.value a.limbs.val + LimbComposition.value b.limbs.val) % (2^256) := by
  obtain ⟨⟨out, carry⟩, hl, hb⟩ := add_loop_balance 4 a b false 0#usize (by simp)
  refine ⟨out, wrapping_add_from_loop a b out carry hl, ?_⟩
  change LimbComposition.value out.limbs.val + radix ^ 4 * carry.toNat = _ at hb
  simp at hb
  have hbound := LimbComposition.value_bound out.limbs.val
  have hlen : out.limbs.val.length = 4 := by simp
  rw [hlen] at hbound
  have hm := congrArg (fun n : Nat => n % radix ^ 4) hb
  have he : LimbComposition.value out.limbs.val =
      (LimbComposition.value a.limbs.val + LimbComposition.value b.limbs.val) % radix ^ 4 := by
    simpa [Nat.add_mod, Nat.mul_mod, Nat.mod_eq_of_lt hbound] using hm
  simpa [LimbComposition.four_limbs_radix] using he

theorem add_all_inputs (a b : Word) :
    ∃ out, U256Extracted.add a b = .ok out ∧
      LimbComposition.value out.limbs.val =
        (LimbComposition.value a.limbs.val + LimbComposition.value b.limbs.val) % (2^256) := by
  exact wrapping_add_all_inputs a b

theorem sub_body_balance (self rhs : Word) (borrow : Bool) (iter : Usize)
    (bound : iter.val < 4) :
    ∃ next : Word × Bool × Usize,
      U256Extracted.ruint.add.Uint.overflowing_sub_loop.body rhs self borrow iter =
        .ok (.cont next) ∧ next.2.2.val = iter.val + 1 ∧
      LimbComposition.value self.limbs.val + radix ^ (iter.val + 1) * next.2.1.toNat =
        LimbComposition.value next.1.limbs.val +
          radix ^ iter.val * ((rhs.limbs.val[iter.val]'(by simpa using bound)).val + borrow.toNat) := by
  have hs : iter.val < self.limbs.length := by simpa using bound
  have hr : iter.val < rhs.limbs.length := by simpa using bound
  obtain ⟨next, hn, hv, _⟩ := U256Correspondence.range_increment_defined iter bound
  obtain ⟨left, hl, hleft⟩ := WP.spec_imp_exists (Array.index_usize_spec self.limbs iter hs)
  obtain ⟨right, hr, hright⟩ := WP.spec_imp_exists (Array.index_usize_spec rhs.limbs iter hr)
  obtain ⟨⟨lo, out⟩, hw, hword⟩ := U256Correspondence.borrowing_sub_all_inputs left right borrow
  obtain ⟨limbs, hu, hlimbs⟩ := WP.spec_imp_exists (Array.update_spec self.limbs iter lo hs)
  refine ⟨(⟨limbs⟩, out, next), ?_, hv, ?_⟩
  · simp [U256Extracted.ruint.add.Uint.overflowing_sub_loop.body,
      UScalar.lt_equiv, bound, hn, hl, hr, hw, hu]
  · have hset := value_set_balance self.limbs.val iter.val lo hs
    have hflag : left.val + radix * out.toNat = lo.val + right.val + borrow.toNat := by
      cases he : out <;> simpa [he] using hword
    have hvals : limbs.val = self.limbs.val.set iter.val lo := by
      simp [hlimbs]
    change LimbComposition.value self.limbs.val + radix ^ (iter.val + 1) * out.toNat =
      LimbComposition.value limbs.val + _
    rw [hvals]
    rw [hleft, hright] at hflag
    simp only [pow_succ]
    have scaled := congrArg (fun n : Nat => radix ^ iter.val * n) hflag
    nlinarith only [hset, scaled]

theorem sub_loop_balance (fuel : Nat) (self rhs : Word) (borrow : Bool) (iter : Usize)
    (remaining : iter.val + fuel = 4) :
    ∃ result : Word × Bool,
      U256Extracted.ruint.add.Uint.overflowing_sub_loop self rhs borrow iter = .ok result ∧
      LimbComposition.value self.limbs.val + radix ^ 4 * result.2.toNat =
        LimbComposition.value result.1.limbs.val +
          radix ^ iter.val * (LimbComposition.value (rhs.limbs.val.drop iter.val) + borrow.toNat) := by
  induction fuel generalizing self borrow iter with
  | zero =>
    have hi : iter.val = 4 := by omega
    refine ⟨(self, borrow), ?_, ?_⟩
    · rw [U256Extracted.ruint.add.Uint.overflowing_sub_loop, loop.eq_1]
      simp [U256Extracted.ruint.add.Uint.overflowing_sub_loop.body, UScalar.lt_equiv, hi]
    · simp [hi, LimbComposition.value]
  | succ fuel ih =>
    have hb : iter.val < 4 := by omega
    obtain ⟨⟨self1, borrow1, iter1⟩, hs, hv, hbal⟩ := sub_body_balance self rhs borrow iter hb
    change iter1.val = iter.val + 1 at hv
    change LimbComposition.value self.limbs.val + radix ^ (iter.val + 1) * borrow1.toNat =
      LimbComposition.value self1.limbs.val + _ at hbal
    obtain ⟨result, hr, hresult⟩ := ih self1 borrow1 iter1 (by omega)
    refine ⟨result, ?_, ?_⟩
    · rw [U256Extracted.ruint.add.Uint.overflowing_sub_loop, loop.eq_1]
      simp only [hs, bind_ok]
      exact hr
    · have hdrop := value_drop_step rhs.limbs.val iter.val (by simpa using hb)
      rw [hv] at hresult
      rw [hdrop]
      simp only [pow_succ] at hbal hresult
      nlinarith only [hbal, hresult]

theorem wrapping_sub_from_loop (self rhs out : Word) (borrow : Bool)
    (hl : U256Extracted.ruint.add.Uint.overflowing_sub_loop self rhs false 0#usize = .ok (out, borrow)) :
    U256Extracted.ruint.add.Uint.wrapping_sub self rhs = .ok out := by
  obtain ⟨last, hi, _⟩ := WP.spec_imp_exists
    (Array.index_usize_spec out.limbs 3#usize (by simp))
  have hs : (4#usize - 1#usize : Result Usize) = .ok 3#usize := by
    obtain ⟨out, ho, hv⟩ := WP.spec_imp_exists
      (@UScalar.sub_spec .Usize 4#usize 1#usize (by simp))
    have he : out = 3#usize := by
      apply UScalar.eq_of_val_eq
      simpa using hv
    simpa [he] using ho
  simp [U256Extracted.ruint.add.Uint.wrapping_sub,
    U256Extracted.ruint.add.Uint.overflowing_sub, hl, hs, hi,
    U256Correspondence.mask256, U256Correspondence.masked256_identity]


theorem wrapping_sub_all_inputs (a b : Word) :
    ∃ out, U256Extracted.ruint.add.Uint.wrapping_sub a b = .ok out ∧
      LimbComposition.value out.limbs.val =
        (LimbComposition.value a.limbs.val + 2^256 - LimbComposition.value b.limbs.val) % (2^256) := by
  obtain ⟨⟨out, borrow⟩, hl, hb⟩ := sub_loop_balance 4 a b false 0#usize (by simp)
  refine ⟨out, wrapping_sub_from_loop a b out borrow hl, ?_⟩
  change LimbComposition.value a.limbs.val + radix ^ 4 * borrow.toNat = _ at hb
  simp at hb
  have hbound := LimbComposition.value_bound out.limbs.val
  have hlen : out.limbs.val.length = 4 := by simp
  rw [hlen, LimbComposition.four_limbs_radix] at hbound
  rw [LimbComposition.four_limbs_radix] at hb
  cases he : borrow
  · simp only [he, Bool.toNat_false, Nat.mul_zero, Nat.add_zero] at hb
    have hd : LimbComposition.value a.limbs.val + 2^256 - LimbComposition.value b.limbs.val =
        LimbComposition.value out.limbs.val + 2^256 := by omega
    rw [hd]
    simp only [Nat.add_mod, Nat.mod_self, Nat.add_zero, Nat.mod_eq_of_lt hbound]
  · simp only [he, Bool.toNat_true, Nat.mul_one] at hb
    have hd : LimbComposition.value a.limbs.val + 2^256 - LimbComposition.value b.limbs.val =
        LimbComposition.value out.limbs.val := by omega
    rw [hd, Nat.mod_eq_of_lt hbound]

theorem sub_all_inputs (a b : Word) :
    ∃ out, U256Extracted.sub a b = .ok out ∧
      LimbComposition.value out.limbs.val =
        (LimbComposition.value a.limbs.val + 2^256 - LimbComposition.value b.limbs.val) % (2^256) := by
  exact wrapping_sub_all_inputs a b

end U256Arithmetic
