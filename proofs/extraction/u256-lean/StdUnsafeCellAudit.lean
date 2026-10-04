import StdUnsafeCellCorrespondence
import Lean
open Lean in
run_cmd do
  let env ← getEnv
  for entry in env.constants.toList do
    if (`StdUnsafeCellPointer).isPrefixOf entry.1 then
      let axioms ← collectAxioms entry.1
      for axiomName in axioms do
        unless #[`propext, `Quot.sound, `Classical.choice].contains axiomName do
          throwError "{entry.1} depends on forbidden axiom {axiomName}"
  for theoremName in #[`StdUnsafeCellPointer.actual_effect_chain,
      `StdUnsafeCellPointer.actual_identity_conditional,
      `StdUnsafeCellPointer.identity_model,
      `StdUnsafeCellPointer.conditional_alias_observation] do
    match env.find? theoremName with
    | some (.thmInfo _) => pure ()
    | _ => throwError "Missing expected pointer IR theorem {theoremName}"
  logInfo "Four actual-source pointer IR theorems audited; physical Rust layout/cast/retag/provenance and Cell/IC correspondence unproved."
