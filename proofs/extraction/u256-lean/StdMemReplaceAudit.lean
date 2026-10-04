import StdMemReplaceCorrespondence
import Lean
open Lean in
run_cmd do
  let env ← getEnv
  let mut checked : Nat := 0
  for entry in env.constants.toList do
    if (`StdMemReplaceState).isPrefixOf entry.1 then
      let axioms ← collectAxioms entry.1
      for axiomName in axioms do
        unless #[`propext, `Quot.sound, `Classical.choice].contains axiomName do
          throwError "{entry.1} depends on forbidden axiom {axiomName}"
  for theoremName in #[`StdMemReplaceState.actual_replace_scalar_heap,
      `StdMemReplaceState.denied_read_preserves_heap,
      `StdMemReplaceState.denied_write_preserves_heap,
      `StdMemReplaceState.uninitialized_preserves_heap,
      `StdMemReplaceState.alias_observes_write,
      `StdMemReplaceState.other_address_preserved,
      `StdMemReplaceState.actual_replace_alias_and_frame] do
    match env.find? theoremName with
    | some (.thmInfo _) => checked := checked + 1
    | _ => throwError "Missing expected conditional IR theorem {theoremName}"
  if checked != 7 then throwError "Expected seven conditional IR theorem declarations, got {checked}"
  logInfo "Seven scalar heap IR theorems audited; Rust/retag/provenance/Cell/IC correspondence remains unproved."
