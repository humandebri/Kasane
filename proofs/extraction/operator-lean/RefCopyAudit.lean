import RefCopyFragment
import Lean

open Lean in
run_cmd do
  let env ← getEnv
  let mut checked : Nat := 0
  for entry in env.constants.toList do
    let name := entry.1
    if (`RefCopyFragment).isPrefixOf name then
      let axioms ← collectAxioms name
      for axiomName in axioms do
        unless #[`propext, `Quot.sound, `Classical.choice].contains axiomName do
          throwError "{name} depends on forbidden axiom {axiomName}"
      if entry.2.isTheorem then
        checked := checked + 1
        logInfo m!"{name}: {axioms}"
  if checked == 0 then throwError "No fragment theorems checked"
  logInfo m!"Fragment model axiom audit passed ({checked} theorem declarations)."
