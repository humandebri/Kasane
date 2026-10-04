import SharedArrayPointerFootprint
import Lean
open Lean in
run_cmd do
  let env ← getEnv
  for entry in env.constants.toList do
    if (`SharedArrayPointerFootprint).isPrefixOf entry.1 then
      let axioms ← collectAxioms entry.1
      for name in axioms do
        unless #[`propext, `Quot.sound, `Classical.choice].contains name do
          throwError "{entry.1} depends on forbidden axiom {name}"
  for name in #[`SharedArrayPointerFootprint.retain_address,
      `SharedArrayPointerFootprint.retain_provenance,
      `SharedArrayPointerFootprint.retain_offset,
      `SharedArrayPointerFootprint.retain_read_footprint,
      `SharedArrayPointerFootprint.retain_alignment,
      `SharedArrayPointerFootprint.retain_write_permission,
      `SharedArrayPointerFootprint.positive_count_first_element_footprint,
      `SharedArrayPointerFootprint.empty_extent_positive_read_rejected,
      `SharedArrayPointerFootprint.zero_size_footprint,
      `SharedArrayPointerFootprint.revoke_retains_nonnull_value,
      `SharedArrayPointerFootprint.revoked_positive_read_rejected,
      `SharedArrayPointerFootprint.same_loan_aliases_resolve_equally,
      `SharedArrayPointerFootprint.equal_address_does_not_imply_equal_provenance,
      `SharedArrayPointerFootprint.equal_symbolic_content_id_does_not_force_equal_address] do
    match env.find? name with
    | some (.thmInfo _) => pure ()
    | _ => throwError "Missing expected theorem {name}"
  logInfo "Fourteen pointer-footprint MODEL theorems audited; Rust/LLVM and origin-resolution refinement remain unproved."
