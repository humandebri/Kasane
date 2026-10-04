import Lake
open Lake DSL
require «kasane-rust-correspondence» from "../lean"
package «kasane-u256-correspondence-experimental»
@[default_target] lean_lib U256Generated
@[default_target] lean_lib U256Correspondence
@[default_target] lean_lib U256Loops

@[default_target] lean_lib U256Arithmetic

@[default_target] lean_lib RevmGasGenerated
@[default_target] lean_lib RevmGasCorrespondence

@[default_target] lean_lib RevmResultGenerated
@[default_target] lean_lib RevmResultCorrespondence

@[default_target] lean_lib LedgerTimeGenerated
@[default_target] lean_lib LedgerTimeCorrespondence

@[default_target] lean_lib LedgerTimeWrappingGenerated
@[default_target] lean_lib LedgerTimeWrappingCorrespondence

@[default_target] lean_lib BitwiseGenerated
@[default_target] lean_lib BitwiseCorrespondence

@[default_target] lean_lib MonoLifetimeGenerated
@[default_target] lean_lib MonoLifetimeCorrespondence
@[default_target] lean_lib MonoLifetimeAudit

@[default_target] lean_lib RevmJumpdestGenerated
@[default_target] lean_lib RevmJumpdestCorrespondence
@[default_target] lean_lib RevmJumpdestAudit
