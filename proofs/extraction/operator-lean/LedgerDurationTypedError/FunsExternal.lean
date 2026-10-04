import LedgerDurationTypedError.Types
open Aeneas Aeneas.Std
namespace LedgerDurationTypedError
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
def core.result.Result.unwrap {T E : Type}
    (_ : core.fmt.Debug E) (r : Aeneas.Std.core.result.Result T E) : Result T :=
  match r with
  | .Ok x => .ok x
  | .Err _ => .fail .panic

end LedgerDurationTypedError
