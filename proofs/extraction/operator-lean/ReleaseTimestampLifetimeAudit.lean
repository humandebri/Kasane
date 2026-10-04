import ReleaseTimestampLifetimeCorrespondence
import Lean
open Lean in
run_cmd do
  let env ← getEnv
  for entry in env.constants.toList do
    if (`ReleaseTimestampLocalLifetime).isPrefixOf entry.1 then
      let axioms ← collectAxioms entry.1
      for axiomName in axioms do
        unless #[`propext, `Quot.sound, `Classical.choice].contains axiomName do
          throwError "{entry.1} depends on forbidden axiom {axiomName}"
  for theoremName in #[`ReleaseTimestampLocalLifetime.check_append,
      `ReleaseTimestampLocalLifetime.checked_prefix,
      `ReleaseTimestampLocalLifetime.dead_read_rejected,
      `ReleaseTimestampLocalLifetime.live_resets_initialization,
      `ReleaseTimestampLocalLifetime.repeated_dead,
      `ReleaseTimestampLocalLifetime.same_lifetime_schedule,
      `ReleaseTimestampLocalLifetime.add_event_count,
      `ReleaseTimestampLocalLifetime.sub_event_count,
      `ReleaseTimestampLocalLifetime.add_schedule_checked,
      `ReleaseTimestampLocalLifetime.sub_schedule_checked,
      `ReleaseTimestampLocalLifetime.add_all_prefixes_checked,
      `ReleaseTimestampLocalLifetime.sub_all_prefixes_checked,
      `ReleaseTimestampLocalLifetime.premature_argument_death_rejected,
      `ReleaseTimestampLocalLifetime.uninitialized_return_rejected,
      `ReleaseTimestampLocalLifetime.double_move_rejected,
      `ReleaseTimestampLocalLifetime.lifetime_restart_read_rejected] do
    match env.find? theoremName with
    | some (.thmInfo _) => pure ()
    | _ => throwError "Missing expected theorem {theoremName}"
  logInfo "Sixteen local lifetime MODEL theorems audited; Rust memory/ref/provenance, call and compiler/IC refinement remain unproved."
