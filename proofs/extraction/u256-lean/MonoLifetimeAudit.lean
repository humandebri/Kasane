import MonoLifetimeCorrespondence
import Lean
open Lean in
run_cmd do
  let env ← getEnv
  for entry in env.constants.toList do
    if (`MonoLifetimeCorrespondence).isPrefixOf entry.1 || (`MonoLifetimeGeneric).isPrefixOf entry.1 then
      let axioms ← collectAxioms entry.1
      for axiomName in axioms do
        unless #[`propext, `Quot.sound, `Classical.choice].contains axiomName do
          throwError "{entry.1} depends on forbidden axiom {axiomName}"
  for theoremName in #[`MonoLifetimeCorrespondence.same_noop_exact,
      `MonoLifetimeCorrespondence.split_noop_exact,
      `MonoLifetimeCorrespondence.split_left_exact,
      `MonoLifetimeCorrespondence.split_right_exact,
      `MonoLifetimeCorrespondence.left_update_selected,
      `MonoLifetimeCorrespondence.left_update_preserves_other,
      `MonoLifetimeCorrespondence.right_update_selected,
      `MonoLifetimeCorrespondence.right_update_preserves_other] do
    match env.find? theoremName with
    | some (.thmInfo _) => pure ()
    | _ => throwError "Missing expected theorem {theoremName}"
  logInfo "Eight generated lifetime-only fixture theorems audited; EVM and Rust heap refinement remain unproved."
