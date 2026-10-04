import StdCellGetCorrespondence
import Lean
open Lean in
run_cmd do
  let env ← getEnv
  for entry in env.constants.toList do
    if (`StdCellGetState).isPrefixOf entry.1 then
      let axioms ← collectAxioms entry.1
      for axiomName in axioms do
        unless #[`propext, `Quot.sound, `Classical.choice].contains axiomName do
          throwError "{entry.1} depends on forbidden axiom {axiomName}"
  for theoremName in #[`StdCellGetState.actual_effect_chain,
      `StdCellGetState.bind_pointer_chain,
      `StdCellGetState.linked_five_effects,
      `StdCellGetState.linked_read_conditional,
      `StdCellGetState.linked_denied_read_conditional,
      `StdCellGetState.projection_failure_preserved] do
    match env.find? theoremName with
    | some (.thmInfo _) => pure ()
    | _ => throwError "Missing expected composed Cell IR theorem {theoremName}"
  logInfo "Five Cell get IR theorems and one composition helper audited; Rust layout/ref/retag/read/unwind/Cell/IC correspondence unproved."
