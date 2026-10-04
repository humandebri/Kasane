import DurationReadRefinement
import Lean
open Lean in
run_cmd do
  let env ← getEnv
  let mut checked : Nat := 0
  for entry in env.constants.toList do
    let declName := entry.1
    if (`DurationReadRefinement).isPrefixOf declName ||
        (`DurationReadCorrespondence).isPrefixOf declName ||
        (`DurationRead).isPrefixOf declName then
      let axioms ← collectAxioms declName
      for axiomName in axioms do
        unless #[`propext, `Quot.sound, `Classical.choice].contains axiomName do
          throwError "{declName} depends on forbidden axiom {axiomName}"
      if entry.2.isTheorem then
        checked := checked + 1
        logInfo m!"{declName}: {axioms}"
  if checked != 24 then throwError "Expected 24 source read/refinement declarations, got {checked}"
  logInfo m!"Range-preserving source audit passed ({checked} theorem declarations)."
