import StdBorrowCounterCorrespondence
import Lean
open Lean in
run_cmd do
  let env ← getEnv
  for entry in env.constants.toList do
    if (`StdBorrowCounterCorrespondence).isPrefixOf entry.1 || (`StdBorrowCounter).isPrefixOf entry.1 then
      let axioms ← collectAxioms entry.1
      for axiomName in axioms do
        unless #[`propext, `Quot.sound, `Classical.choice].contains axiomName do
          throwError "{entry.1} depends on forbidden axiom {axiomName}"
  for theoremName in #[`StdBorrowCounterCorrespondence.reading_all_inputs,
      `StdBorrowCounterCorrespondence.writing_all_inputs,
      `StdBorrowCounterCorrespondence.mutually_exclusive,
      `StdBorrowCounterCorrespondence.unused_classification] do
    match env.find? theoremName with
    | some (.thmInfo _) => pure ()
    | _ => throwError "Missing expected theorem {theoremName}"
  logInfo "Four generated actual-std counter theorems audited; RefCell/Ref heap, source/extractor/compiler and IC correspondence remain unproved."
