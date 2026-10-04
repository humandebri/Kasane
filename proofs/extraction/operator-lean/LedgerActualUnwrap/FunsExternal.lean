import LedgerActualUnwrap.Types
open Aeneas Aeneas.Std
namespace LedgerActualUnwrap
-- Explicit observation model of Debug/unwrap, excluding formatting and panic text.
-- These names deliberately shadow the backend's axiomatic Formatter interface.
abbrev core.fmt.Formatter := Unit
structure core.fmt.Debug (E : Type) where
  fmt : E → core.fmt.Formatter → Result
    ((Aeneas.Std.core.result.Result Unit Aeneas.Std.core.fmt.Error) × core.fmt.Formatter)
def core.num.error.TryFromIntError.Insts.CoreFmtDebug.fmt
    (_ : core.num.error.TryFromIntError) (f : core.fmt.Formatter) :
    Result ((Aeneas.Std.core.result.Result Unit Aeneas.Std.core.fmt.Error) ×
      core.fmt.Formatter) := .ok (.Ok (), f)
-- Conditional local observation: formatting and unwinding are outside this model.
def core.result.unwrap_failed (_ : Str) (_ : Dyn (fun E => core.fmt.Debug E)) :
    Result Never := .fail .panic
end LedgerActualUnwrap
