import RevmExecuteCorrespondence
import Lean
open Lean in
run_cmd do
  let env ← getEnv
  for entry in env.constants.toList do
    if (`RevmExecuteCorrespondence).isPrefixOf entry.1 || (`RevmExecuteDynamic).isPrefixOf entry.1 then
      let axioms ← collectAxioms entry.1
      for axiomName in axioms do
        unless #[`propext, `Quot.sound, `Classical.choice].contains axiomName do
          throwError "{entry.1} depends on forbidden axiom {axiomName}"
  for theoremName in #[`RevmExecuteCorrespondence.body_is_callback,
      `RevmExecuteCorrespondence.outcomes_iff, `RevmExecuteCorrespondence.static_gas_independent] do
    match env.find? theoremName with
    | some (.thmInfo _) => pure ()
    | _ => throwError "Missing expected theorem {theoremName}"
  logInfo "Three generated actual-execute body theorems audited; Rust callback/heap/trait erasure/compiler/EVM/IC correspondence remains unproved."
