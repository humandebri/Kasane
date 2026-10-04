import RevmJumpdestCorrespondence
import Lean
open Lean in
run_cmd do
  let env ← getEnv
  for entry in env.constants.toList do
    if (`RevmJumpdestCorrespondence).isPrefixOf entry.1 || (`RevmJumpdestTransparent).isPrefixOf entry.1 then
      let axioms ← collectAxioms entry.1
      for axiomName in axioms do
        unless #[`propext, `Quot.sound, `Classical.choice].contains axiomName do
          throwError "{entry.1} depends on forbidden axiom {axiomName}"
  for theoremName in #[`RevmJumpdestCorrespondence.body_identity,
      `RevmJumpdestCorrespondence.observations_preserved] do
    match env.find? theoremName with
    | some (.thmInfo _) => pure ()
    | _ => throwError "Missing expected theorem {theoremName}"
  logInfo "Two generated actual-JUMPDEST body theorems audited; trait erasure/Rust/EVM dispatch/heap/compiler correspondence remains unproved."
