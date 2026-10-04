import ReleaseTimestampCorrespondence
import Lean
open Lean in
run_cmd do
  let env ← getEnv
  for entry in env.constants.toList do
    if (`ReleaseTimestampSequence).isPrefixOf entry.1 then
      let axioms ← collectAxioms entry.1
      for axiomName in axioms do
        unless #[`propext, `Quot.sound, `Classical.choice].contains axiomName do
          throwError "{entry.1} depends on forbidden axiom {axiomName}"
  for theoremName in #[`ReleaseTimestampSequence.calls_eq,
      `ReleaseTimestampSequence.model_add_for, `ReleaseTimestampSequence.model_sub_for,
      `ReleaseTimestampSequence.model_add, `ReleaseTimestampSequence.model_sub,
      `ReleaseTimestampSequence.release_add_conditional,
      `ReleaseTimestampSequence.release_sub_conditional,
      `ReleaseTimestampSequence.release_add_success,
      `ReleaseTimestampSequence.release_sub_success,
      `ReleaseTimestampSequence.release_add_overflow,
      `ReleaseTimestampSequence.release_sub_overflow] do
    match env.find? theoremName with
    | some (.thmInfo _) => pure ()
    | _ => throwError "Missing expected theorem {theoremName}"
  logInfo "Eleven conditional restricted-sequence theorems audited; full Rust MIR/std-library/compiler/IC refinement remains unproved."
