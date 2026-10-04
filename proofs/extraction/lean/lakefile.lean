import Lake
open Lake DSL
require aeneas from "../../../.local/proof-tools/aeneas/backends/lean"
require «kasane-evm-proofs» from "../../evm"
package «kasane-rust-correspondence»
@[default_target] lean_lib KasaneExtracted
@[default_target] lean_lib Correspondence
@[default_target] lean_lib KasaneWordExtracted
@[default_target] lean_lib WordCorrespondence
@[default_target] lean_lib LimbComposition
