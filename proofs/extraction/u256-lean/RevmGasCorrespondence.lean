import RevmGasGenerated
open Aeneas Aeneas.Std
namespace RevmGasCorrespondence
abbrev Gas := RevmGas.revm_interpreter.gas.Gas

theorem record_cost_all_inputs (gas : Gas) (cost : U64) :
    ∃ result : Bool × Gas,
      RevmGas.revm_interpreter.gas.Gas.record_cost gas cost = .ok result ∧
      (result.1 = true ↔ cost.val ≤ gas.remaining.val) ∧
      result.2.remaining.val = (if cost.val ≤ gas.remaining.val then gas.remaining.val - cost.val else gas.remaining.val) ∧
      result.2.limit = gas.limit ∧ result.2.refunded = gas.refunded ∧ result.2.memory = gas.memory := by
  have hs := U64.checked_sub_bv_spec gas.remaining cost
  cases he : U64.checked_sub gas.remaining cost with
  | none =>
    rw [he] at hs
    refine ⟨(false, gas), ?_, ?_, ?_, rfl, rfl, rfl⟩
    · simp [RevmGas.revm_interpreter.gas.Gas.record_cost, he, lift]
    · simp; omega
    · simp only [if_neg (by omega : ¬ cost.val ≤ gas.remaining.val)]
  | some remaining =>
    rw [he] at hs
    refine ⟨(true, { gas with remaining := remaining }), ?_, ?_, ?_, rfl, rfl, rfl⟩
    · simp [RevmGas.revm_interpreter.gas.Gas.record_cost, he, lift]
    · simp; exact hs.1
    · simp only [if_pos hs.1]; exact hs.2.1

theorem record_cost_unsafe_all_inputs (gas : Gas) (cost : U64) :
    ∃ result : Bool × Gas,
      RevmGas.revm_interpreter.gas.Gas.record_cost_unsafe gas cost = .ok result ∧
      (result.1 = true ↔ gas.remaining.val < cost.val) ∧
      result.2.remaining.val = (gas.remaining.val + (2^64 - cost.val)) % (2^64) ∧
      result.2.limit = gas.limit ∧ result.2.refunded = gas.refunded ∧ result.2.memory = gas.memory := by
  refine ⟨(gas.remaining < cost, { gas with remaining := core.num.U64.wrapping_sub gas.remaining cost }), ?_, ?_, ?_, rfl, rfl, rfl⟩
  · simp [RevmGas.revm_interpreter.gas.Gas.record_cost_unsafe, lift]
  · simp [UScalar.lt_equiv]
  · simp [U64.size, U64.numBits]

end RevmGasCorrespondence
