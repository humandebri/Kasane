import LedgerDurationFullRefinement
import Lean
open Lean in
run_cmd do
  let env ← getEnv
  let mut checked : Nat := 0
  for entry in env.constants.toList do
    let declName := entry.1
    if (`LedgerDurationFullCorrespondence).isPrefixOf declName ||
        (`LedgerDurationFull).isPrefixOf declName ||
        (`LedgerDurationFullRefinement).isPrefixOf declName then
      let axioms ← collectAxioms declName
      for axiomName in axioms do
        unless #[`propext, `Quot.sound, `Classical.choice].contains axiomName do
          throwError "{declName} depends on forbidden axiom {axiomName}"
      if entry.2.isTheorem then
        checked := checked + 1
        logInfo m!"{declName}: {axioms}"
  if checked != 86 then throwError "Expected 86 full-range Duration declarations, got {checked}"
  logInfo m!"Duration source audit passed ({checked} theorem declarations)."
