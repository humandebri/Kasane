import RevmResultGenerated
open Aeneas Aeneas.Std
namespace RevmResultCorrespondence
open RevmResult.revm_interpreter.instruction_result

theorem classification_all_inputs (result : InstructionResult) :
    ∃ ok revert error okOrRevert : Bool,
      InstructionResult.is_ok result = .ok ok ∧
      InstructionResult.is_revert result = .ok revert ∧
      InstructionResult.is_error result = .ok error ∧
      InstructionResult.is_ok_or_revert result = .ok okOrRevert ∧
      ok.toNat + revert.toNat + error.toNat = 1 ∧
      okOrRevert = (ok || revert) := by
  cases result <;> first
    | exact ⟨true, false, false, true, rfl, rfl, rfl, rfl, rfl, rfl⟩
    | exact ⟨false, true, false, true, rfl, rfl, rfl, rfl, rfl, rfl⟩
    | exact ⟨false, false, true, false, rfl, rfl, rfl, rfl, rfl, rfl⟩

theorem success_exact (result : InstructionResult) :
    InstructionResult.is_ok result = .ok true ↔
      result = .Stop ∨ result = .Return ∨ result = .SelfDestruct := by
  cases result <;> simp [InstructionResult.is_ok]

theorem revert_exact (result : InstructionResult) :
    InstructionResult.is_revert result = .ok true ↔
      result = .Revert ∨ result = .CallTooDeep ∨ result = .OutOfFunds ∨
      result = .CreateInitCodeStartingEF00 ∨ result = .InvalidEOFInitCode ∨
      result = .InvalidExtDelegateCallTarget := by
  cases result <;> simp [InstructionResult.is_revert]

end RevmResultCorrespondence
