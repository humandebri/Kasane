import KasaneEvm
import Lean

open Lean in
run_cmd do
  let env ← getEnv
  let mut checked := 0
  for (name, info) in env.constants.toList do
    if (`KasaneEvm).isPrefixOf name then
      let axioms ← collectAxioms name
      for axiomName in axioms do
        unless #[`propext, `Quot.sound, `Classical.choice].contains axiomName do
          throwError "{name} depends on forbidden axiom {axiomName}"
      if info.isTheorem then
        checked := checked + 1
        logInfo m!"{name}: {axioms}"
  if checked == 0 then throwError "No KasaneEvm theorems checked"
  logInfo m!"Axiom audit passed ({checked} theorem declarations, including generated lemmas)."
