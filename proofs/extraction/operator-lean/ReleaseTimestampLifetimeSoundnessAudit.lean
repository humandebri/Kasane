import ReleaseTimestampLifetimeSoundness
import Lean
open Lean in
run_cmd do
  let env ← getEnv
  for entry in env.constants.toList do
    if (`ReleaseTimestampLifetimeSoundness).isPrefixOf entry.1 then
      let axioms ← collectAxioms entry.1
      for axiomName in axioms do
        unless #[`propext, `Quot.sound, `Classical.choice].contains axiomName do
          throwError "{entry.1} depends on forbidden axiom {axiomName}"
  for theoremName in #[`ReleaseTimestampLifetimeSoundness.step_sound,
      `ReleaseTimestampLifetimeSoundness.check_sound,
      `ReleaseTimestampLifetimeSoundness.add_trace_safe,
      `ReleaseTimestampLifetimeSoundness.sub_trace_safe,
      `ReleaseTimestampLifetimeSoundness.add_prefix_safe,
      `ReleaseTimestampLifetimeSoundness.sub_prefix_safe] do
    match env.find? theoremName with
    | some (.thmInfo _) => pure ()
    | _ => throwError "Missing expected theorem {theoremName}"
  logInfo "Six lifetime-checker soundness MODEL theorems audited; Rust memory/provenance and execution refinement remain unproved."
