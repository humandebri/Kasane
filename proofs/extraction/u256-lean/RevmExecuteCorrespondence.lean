import RevmExecuteGenerated
open Aeneas Aeneas.Std
namespace RevmExecuteCorrespondence
open RevmExecuteDynamic.revm_interpreter
variable {W H S M B R I F E O : Type}
variable (instruction : instructions.Instruction W H S M B R I F E O)
variable (context : instruction_context.InstructionContext H W S M B R I F E O)

-- Generated actual body only; Rust heap, callback semantics and tool correctness remain obligations.
theorem body_is_callback :
    instructions.Instruction.execute instruction context = instruction.fn_ context := by
  simp [instructions.Instruction.execute]

theorem outcomes_iff
    (result : Result (instruction_context.InstructionContext H W S M B R I F E O)) :
    instructions.Instruction.execute instruction context = result ↔ instruction.fn_ context = result := by
  rw [body_is_callback]

theorem static_gas_independent (gas : U64) :
    instructions.Instruction.execute { instruction with static_gas := gas } context =
      instructions.Instruction.execute instruction context := by
  simp [instructions.Instruction.execute]
end RevmExecuteCorrespondence
