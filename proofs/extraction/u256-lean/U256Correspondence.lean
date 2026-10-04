import U256Generated
import WordCorrespondence
open Aeneas Aeneas.Std
namespace U256Correspondence

theorem carrying_add_all_inputs (a b : U64) (carry : Bool) :
    ∃ result : U64 × Bool,
      U256Extracted.ruint.algorithms.add.carrying_add a b carry = .ok result ∧
      result.1.val + (if result.2 then WordCorrespondence.radix else 0) =
        a.val + b.val + carry.toNat := by
  exact WordCorrespondence.carrying_add_all_inputs a b carry

theorem borrowing_sub_all_inputs (a b : U64) (borrow : Bool) :
    ∃ result : U64 × Bool,
      U256Extracted.ruint.algorithms.add.borrowing_sub a b borrow = .ok result ∧
      a.val + (if result.2 then WordCorrespondence.radix else 0) =
        result.1.val + b.val + borrow.toNat := by
  exact WordCorrespondence.borrowing_sub_all_inputs a b borrow

theorem precondition_defined (a b : Usize)
    (fits : a.val + b.val ≤ UScalar.max .Usize) :
    U256Extracted.core.num.Usize.unchecked_add.precondition_check a b = .ok () := by
  have h := UScalar.overflowing_add_eq a b
  have noOverflow : ¬ a.val + b.val > UScalar.max .Usize := by omega
  simp only [if_neg noOverflow] at h
  have flag : (core.num.Usize.overflowing_add a b).2 = false := h.2
  cases he : core.num.Usize.overflowing_add a b with
  | mk out carry =>
    simp only [he] at flag
    simp [U256Extracted.core.num.Usize.unchecked_add.precondition_check, he, flag, lift]

theorem unchecked_add_defined (a b : Usize)
    (fits : a.val + b.val ≤ UScalar.max .Usize) :
    ∃ result, U256Extracted.core.num.Usize.unchecked_add a b = .ok result ∧
      result.val = a.val + b.val := by
  have h := WP.spec_imp_exists (UScalar.add_spec fits)
  simpa [U256Extracted.core.num.Usize.unchecked_add,
    U256Extracted.core.ub_checks.check_language_ub,
    U256Extracted.core.ub_checks.check_language_ub.runtime] using h

theorem range_increment_defined (i : Usize) (range : i.val < 4) :
    ∃ result, U256Extracted.core.num.Usize.unchecked_add i 1#usize = .ok result ∧
      result.val = i.val + 1 ∧ result.val ≤ 4 := by
  have one : (1#usize).val = 1 := by simp
  have hm : 4 ≤ UScalar.max .Usize := by scalar_tac
  have fits : i.val + (1#usize).val ≤ UScalar.max .Usize := by omega
  obtain ⟨result, hr, hv⟩ := unchecked_add_defined i 1#usize fits
  refine ⟨result, hr, ?_, ?_⟩ <;> omega

theorem mask256 : U256Extracted.ruint.Uint.MASK 256#usize 4#usize =
    .ok core.num.U64.MAX := by
  obtain ⟨rem, hr, hv⟩ := WP.spec_imp_exists
    (@UScalar.rem_spec .Usize 256#usize 64#usize (by simp))
  have hz : rem = 0#usize := by
    apply UScalar.eq_of_val_eq
    simpa using hv
  rw [hz] at hr
  simp [U256Extracted.ruint.Uint.MASK, U256Extracted.ruint.mask, hr]

theorem masked256_identity (self : U256Extracted.ruint.Uint 256#usize 4#usize) :
    U256Extracted.ruint.Uint.masked self = .ok self := by
  simp [U256Extracted.ruint.Uint.masked, U256Extracted.ruint.Uint.apply_mask,
    U256Extracted.ruint.Uint.SHOULD_MASK, mask256]

end U256Correspondence
