import KasaneWordExtracted
open Aeneas Aeneas.Std
namespace WordCorrespondence

def radix : Nat := 2^64

theorem add_balance (a b : U64) :
    (UScalar.overflowing_add a b).1.val +
      (if (UScalar.overflowing_add a b).2 then radix else 0) = a.val + b.val := by
  have h := UScalar.overflowing_add_eq a b
  simp only [UScalar.max, UScalar.size, UScalarTy.numBits, radix] at h ⊢
  split_ifs at h <;> simp_all

theorem sub_balance (a b : U64) :
    a.val + (if (UScalar.overflowing_sub a b).2 then radix else 0) =
      (UScalar.overflowing_sub a b).1.val + b.val := by
  have h := UScalar.overflowing_sub_eq a b
  simp only [UScalar.size, UScalarTy.numBits, radix] at h ⊢
  split_ifs at h <;> simp_all

theorem carrying_add_all_inputs (a b : U64) (carry : Bool) :
    ∃ result : U64 × Bool,
      WordExtracted.ruint.algorithms.add.carrying_add a b carry = .ok result ∧
      result.1.val + (if result.2 then radix else 0) = a.val + b.val + carry.toNat := by
  let first := UScalar.overflowing_add a b
  let bit := UScalar.cast_fromBool .U64 carry
  let second := UScalar.overflowing_add first.1 bit
  refine ⟨(second.1, first.2 || second.2), ?_, ?_⟩
  · cases hf : UScalar.overflowing_add a b with
    | mk lo flag =>
      cases hs : UScalar.overflowing_add lo bit with
      | mk out flag2 =>
        dsimp only [bit] at hs
        simp [WordExtracted.ruint.algorithms.add.carrying_add, lift, first, second,
          core.num.U64.overflowing_add, hf, hs, bit]
  · have h1 := add_balance a b
    have h2 := add_balance first.1 bit
    have hab := a.hBounds
    have hbb := b.hBounds
    have hr := second.1.hBounds
    have hb : bit.val = carry.toNat := UScalar.cast_fromBool_val_eq _ _
    have hc : carry.toNat ≤ 1 := by cases carry <;> decide
    change first.1.val + (if first.2 then radix else 0) = _ at h1
    change second.1.val + (if second.2 then radix else 0) = _ at h2
    dsimp only [first, second] at *
    cases hf : first.2 <;> cases hs : second.2 <;>
      dsimp only [first, second] at hf hs <;>
      simp only [hf, hs, Bool.or_false, Bool.or_true,
        Bool.false_eq_true, ↓reduceIte] at * <;>
      unfold radix at * <;> simp only [UScalarTy.numBits] at * <;> omega

theorem borrowing_sub_all_inputs (a b : U64) (borrow : Bool) :
    ∃ result : U64 × Bool,
      WordExtracted.ruint.algorithms.add.borrowing_sub a b borrow = .ok result ∧
      a.val + (if result.2 then radix else 0) = result.1.val + b.val + borrow.toNat := by
  let first := UScalar.overflowing_sub a b
  let bit := UScalar.cast_fromBool .U64 borrow
  let second := UScalar.overflowing_sub first.1 bit
  refine ⟨(second.1, first.2 || second.2), ?_, ?_⟩
  · cases hf : UScalar.overflowing_sub a b with
    | mk lo flag =>
      cases hs : UScalar.overflowing_sub lo bit with
      | mk out flag2 =>
        dsimp only [bit] at hs
        simp [WordExtracted.ruint.algorithms.add.borrowing_sub, lift, first, second,
          core.num.U64.overflowing_sub, hf, hs, bit]
  · have h1 := sub_balance a b
    have h2 := sub_balance first.1 bit
    have hab := a.hBounds
    have hbb := b.hBounds
    have hr := second.1.hBounds
    have hb : bit.val = borrow.toNat := UScalar.cast_fromBool_val_eq _ _
    have hc : borrow.toNat ≤ 1 := by cases borrow <;> decide
    change a.val + (if first.2 then radix else 0) = _ at h1
    change first.1.val + (if second.2 then radix else 0) = _ at h2
    dsimp only [first, second] at *
    cases hf : first.2 <;> cases hs : second.2 <;>
      dsimp only [first, second] at hf hs <;>
      simp only [hf, hs, Bool.or_false, Bool.or_true,
        Bool.false_eq_true, ↓reduceIte] at * <;>
      unfold radix at * <;> simp only [UScalarTy.numBits] at * <;> omega

end WordCorrespondence
