import U256Arithmetic
import RevmGasCorrespondence
import RevmResultCorrespondence
import LedgerTimeCorrespondence
import LedgerTimeWrappingCorrespondence
import BitwiseCorrespondence
import Lean

open Lean in
run_cmd do
  let env ← getEnv
  let mut checked : Nat := 0
  for entry in env.constants.toList do
    let declName := entry.1
    let declInfo := entry.2
    if (`U256Correspondence).isPrefixOf declName || (`U256Extracted).isPrefixOf declName ||
        (`U256Loops).isPrefixOf declName || (`U256Arithmetic).isPrefixOf declName ||
        (`RevmGas).isPrefixOf declName || (`RevmGasCorrespondence).isPrefixOf declName ||
        (`RevmResult).isPrefixOf declName || (`RevmResultCorrespondence).isPrefixOf declName ||
        (`LedgerTime).isPrefixOf declName || (`LedgerTimeCorrespondence).isPrefixOf declName ||
        (`LedgerTimeWrapping).isPrefixOf declName || (`LedgerTimeWrappingCorrespondence).isPrefixOf declName ||
        (`BitwiseExtracted).isPrefixOf declName || (`BitwiseCorrespondence).isPrefixOf declName then
      let axioms ← collectAxioms declName
      for axiomName in axioms do
        unless #[`propext, `Quot.sound, `Classical.choice].contains axiomName do
          throwError "{declName} depends on forbidden axiom {axiomName}"
      if declInfo.isTheorem then
        checked := checked + 1
        logInfo m!"{declName}: {axioms}"
  if checked == 0 then throwError "No correspondence theorems checked"
  logInfo m!"Axiom audit passed ({checked} theorem declarations, including generated lemmas)."
