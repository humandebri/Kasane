import FnRegionErasure
import Lean
open Lean in
run_cmd do
  let env ← getEnv
  for entry in env.constants.toList do
    if (`FnRegionErasure).isPrefixOf entry.1 then
      let axioms ← collectAxioms entry.1
      for name in axioms do
        unless #[`propext, `Quot.sound, `Classical.choice].contains name do
          throwError "{entry.1} depends on forbidden axiom {name}"
  for name in #[`FnRegionErasure.free_erased, `FnRegionErasure.local_bound_preserved,
      `FnRegionErasure.outer_bound_erased, `FnRegionErasure.erased_preserved,
      `FnRegionErasure.shifts_compose, `FnRegionErasure.erasure_idempotent] do
    match env.find? name with
    | some (.thmInfo _) => pure ()
    | _ => throwError "Missing expected theorem {name}"
  logInfo "Six integer-depth erasure model theorems audited; AST/interpreter correspondence remains unproved."
