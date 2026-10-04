import ReleaseTimestampCFGCorrespondence
import Lean
open Lean in
run_cmd do
  let env ← getEnv
  for entry in env.constants.toList do
    if (`ReleaseTimestampCFG).isPrefixOf entry.1 then
      let axioms ← collectAxioms entry.1
      for axiomName in axioms do
        unless #[`propext, `Quot.sound, `Classical.choice].contains axiomName do
          throwError "{entry.1} depends on forbidden axiom {axiomName}"
  for theoremName in #[`ReleaseTimestampCFG.executeWith_append,
      `ReleaseTimestampCFG.executeWith_invalid,
      `ReleaseTimestampCFG.lower_correct,
      `ReleaseTimestampCFG.add_lower,
      `ReleaseTimestampCFG.sub_lower,
      `ReleaseTimestampCFG.add_graph_equiv,
      `ReleaseTimestampCFG.sub_graph_equiv,
      `ReleaseTimestampCFG.add_graph_conditional,
      `ReleaseTimestampCFG.sub_graph_conditional,
      `ReleaseTimestampCFG.add_insufficient_fuel,
      `ReleaseTimestampCFG.sub_insufficient_fuel] do
    match env.find? theoremName with
    | some (.thmInfo _) => pure ()
    | _ => throwError "Missing expected theorem {theoremName}"
  logInfo "Eleven restricted CFG theorems audited; Rust MIR abstraction, stdlib, compiler and IC refinement remain unproved."
