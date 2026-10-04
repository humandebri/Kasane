import RevmJumpdestGenerated
open Aeneas
namespace RevmJumpdestCorrespondence
open RevmJumpdestTransparent.revm_interpreter
variable {WIRE H Stack Memory Bytecode ReturnData Input RuntimeFlag Extend Output : Type}
abbrev Context := instruction_context.InstructionContext H WIRE Stack Memory Bytecode
  ReturnData Input RuntimeFlag Extend Output
-- All generated context values; source/extractor/compiler and trait-erasure correspondence remain obligations.
theorem body_identity (context : Context (WIRE := WIRE) (H := H) (Stack := Stack)
    (Memory := Memory) (Bytecode := Bytecode) (ReturnData := ReturnData) (Input := Input)
    (RuntimeFlag := RuntimeFlag) (Extend := Extend) (Output := Output)) :
    instructions.control.jumpdest context = .ok context := rfl

theorem observations_preserved {α : Type} (context : Context (WIRE := WIRE) (H := H)
    (Stack := Stack) (Memory := Memory) (Bytecode := Bytecode) (ReturnData := ReturnData)
    (Input := Input) (RuntimeFlag := RuntimeFlag) (Extend := Extend) (Output := Output))
    (observe : Context (WIRE := WIRE) (H := H) (Stack := Stack) (Memory := Memory)
      (Bytecode := Bytecode) (ReturnData := ReturnData) (Input := Input)
      (RuntimeFlag := RuntimeFlag) (Extend := Extend) (Output := Output) → α) :
    (do let next ← instructions.control.jumpdest context; .ok (observe next)) =
      .ok (observe context) := by simp [instructions.control.jumpdest]
end RevmJumpdestCorrespondence
