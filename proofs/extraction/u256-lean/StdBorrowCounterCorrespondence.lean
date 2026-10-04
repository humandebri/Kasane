import StdBorrowCounterGenerated
open Aeneas Aeneas.Std
namespace StdBorrowCounterCorrespondence
open StdBorrowCounter.core.cell

-- Fixed generated std predicates only; cell/Ref heap and source/compiler correspondence remain obligations.
theorem reading_all_inputs (counter : Isize) :
    is_reading counter = .ok (decide (0 < counter.val)) := by
  simp [is_reading, UNUSED, IScalar.lt_equiv]

theorem writing_all_inputs (counter : Isize) :
    is_writing counter = .ok (decide (counter.val < 0)) := by
  simp [is_writing, UNUSED, IScalar.lt_equiv]

theorem mutually_exclusive (counter : Isize) :
    ¬ (is_reading counter = .ok true ∧ is_writing counter = .ok true) := by
  simp only [reading_all_inputs, writing_all_inputs]
  simp
  omega

theorem unused_classification (counter : Isize) :
    (is_reading counter = .ok false ∧ is_writing counter = .ok false) ↔ counter.val = 0 := by
  simp only [reading_all_inputs, writing_all_inputs]
  simp
  omega
end StdBorrowCounterCorrespondence
