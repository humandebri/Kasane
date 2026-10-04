import Lake
open Lake DSL
require aeneas from "../../../.local/proof-tools/aeneas-source/backends/lean"
package «kasane-bitwise-operators-experimental»
@[default_target] lean_lib OperatorsGenerated
@[default_target] lean_lib OperatorCorrespondence

@[default_target] lean_lib StackGenerated
@[default_target] lean_lib StackCorrespondence

@[default_target] lean_lib RefCopyFragment
@[default_target] lean_lib RefCopyAudit

@[default_target] lean_lib TypeInfoGenerated
@[default_target] lean_lib TypeInfoCorrespondence

@[default_target] lean_lib TypeInfoAudit

@[default_target] lean_lib GatBorrowFootprint
@[default_target] lean_lib GatBorrowAudit

@[default_target] lean_lib LedgerDurationExtract
@[default_target] lean_lib LedgerDurationCorrespondence
@[default_target] lean_lib LedgerDurationAudit

@[default_target] lean_lib LedgerDurationSource
@[default_target] lean_lib LedgerDurationSourceCorrespondence
@[default_target] lean_lib LedgerDurationSourceAudit
@[default_target] lean_lib LedgerDurationRefinement
@[default_target] lean_lib LedgerDurationModel

@[default_target] lean_lib DurationRangeGenerated
@[default_target] lean_lib DurationRangeCorrespondence
@[default_target] lean_lib DurationRangeAudit

@[default_target]
lean_lib DurationReadGenerated

@[default_target]
lean_lib DurationReadCorrespondence

@[default_target]
lean_lib DurationReadAudit

@[default_target]
lean_lib DurationReadRefinement

@[default_target]
lean_lib DurationConstructGenerated

@[default_target]
lean_lib DurationConstructCorrespondence

@[default_target]
lean_lib DurationConstructAudit

@[default_target]
lean_lib DurationConstructRefinement

@[default_target]
lean_lib LedgerDurationFullCorrespondence

@[default_target]
lean_lib LedgerDurationFullRefinement

@[default_target]
lean_lib LedgerDurationFullAudit

@[default_target]
lean_lib LedgerDurationFull

@[default_target]
lean_lib LedgerDurationTypedError

@[default_target]
lean_lib LedgerDurationTypedErrorCorrespondence

@[default_target]
lean_lib LedgerDurationTypedErrorRefinement

@[default_target]
lean_lib LedgerDurationTypedErrorAudit

@[default_target] lean_lib LedgerActualUnwrap
@[default_target] lean_lib LedgerActualUnwrapCorrespondence
@[default_target] lean_lib LedgerActualUnwrapRefinement
@[default_target] lean_lib NeverCallCorrespondence
@[default_target] lean_lib LedgerActualUnwrapAudit

@[default_target] lean_lib LedgerUnwrapFailed
@[default_target] lean_lib LedgerUnwrapFailedCorrespondence
@[default_target] lean_lib LedgerUnwrapFailedRefinement
@[default_target] lean_lib DynDebugCorrespondence
@[default_target] lean_lib LedgerUnwrapFailedAudit

@[default_target] lean_lib LedgerPanicPayload
@[default_target] lean_lib LedgerPanicPayloadCorrespondence
@[default_target] lean_lib LedgerPanicPayloadRefinement
@[default_target] lean_lib PanicPayloadCorrespondence
@[default_target] lean_lib LedgerPanicPayloadAudit

@[default_target] lean_lib FnRegionErasure
@[default_target] lean_lib FnRegionErasureAudit

@[default_target] lean_lib SharedArrayPointerFootprint
@[default_target] lean_lib SharedArrayPointerFootprintAudit

@[default_target] lean_lib ReleaseTimestampSequence
@[default_target] lean_lib ReleaseTimestampGenerated
@[default_target] lean_lib ReleaseTimestampCorrespondence
@[default_target] lean_lib ReleaseTimestampAudit

@[default_target] lean_lib ReleaseTimestampCFG
@[default_target] lean_lib ReleaseTimestampCFGGenerated
@[default_target] lean_lib ReleaseTimestampCFGCorrespondence
@[default_target] lean_lib ReleaseTimestampCFGAudit

@[default_target] lean_lib ReleaseTimestampLocalLifetime
@[default_target] lean_lib ReleaseTimestampLifetimeGenerated
@[default_target] lean_lib ReleaseTimestampLifetimeCorrespondence
@[default_target] lean_lib ReleaseTimestampLifetimeAudit

@[default_target] lean_lib ReleaseTimestampLifetimeSoundness
@[default_target] lean_lib ReleaseTimestampLifetimeSoundnessAudit
