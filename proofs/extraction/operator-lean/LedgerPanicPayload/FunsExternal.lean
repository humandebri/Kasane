import LedgerPanicPayload.Types
open Aeneas Aeneas.Std
namespace LedgerPanicPayload
-- Explicit formatting observation interfaces; Rust layout, flags, writing and panic
-- rendering are outside this model. Constructors retain receivers and callbacks.
abbrev core.fmt.Formatter := List U8
structure core.fmt.Debug (E : Type u) where
  fmt : E → core.fmt.Formatter → Result
    ((Aeneas.Std.core.result.Result Unit Aeneas.Std.core.fmt.Error) × core.fmt.Formatter)
structure core.fmt.Display (E : Type u) where
  fmt : E → core.fmt.Formatter → Result
    ((Aeneas.Std.core.result.Result Unit Aeneas.Std.core.fmt.Error) × core.fmt.Formatter)
def core.fmt.DebugShared {E : Type u} (inst : core.fmt.Debug E) : core.fmt.Debug E := inst
def Shared0T.Insts.CoreFmtDisplay.fmt {T : Type u} (inst : core.fmt.Display T)
    (x : T) (f : core.fmt.Formatter) := inst.fmt x f
-- These two foreign formatting bodies remain explicit observations.
def Str.Insts.CoreFmtDisplay.fmt (_ : Str) (f : core.fmt.Formatter) :
    Result ((Aeneas.Std.core.result.Result Unit Aeneas.Std.core.fmt.Error) ×
      core.fmt.Formatter) := .ok (.Ok (), f)
def core.num.error.TryFromIntError.Insts.CoreFmtDebug.fmt
    (_ : core.num.error.TryFromIntError) (f : core.fmt.Formatter) :
    Result ((Aeneas.Std.core.result.Result Unit Aeneas.Std.core.fmt.Error) ×
      core.fmt.Formatter) := .ok (.Ok (), f)
structure core.fmt.rt.Argument where
  -- The closure captures the receiver and the trait implementation together.
  format : core.fmt.Formatter → Result
    ((Aeneas.Std.core.result.Result Unit Aeneas.Std.core.fmt.Error) × core.fmt.Formatter)
def core.fmt.rt.Argument.render (a : core.fmt.rt.Argument) (f : core.fmt.Formatter) :=
  a.format f
def core.fmt.rt.Argument.new_display {T : Type u} (inst : core.fmt.Display T) (x : T) :
    Result core.fmt.rt.Argument := .ok ⟨inst.fmt x⟩
def core.fmt.rt.Argument.new_debug {T : Type u} (inst : core.fmt.Debug T) (x : T) :
    Result core.fmt.rt.Argument := .ok ⟨inst.fmt x⟩
structure core.fmt.Arguments where
  template : List U8
  arguments : List core.fmt.rt.Argument
def core.fmt.Arguments.new {N M : Usize} (bytes : Std.Array U8 N)
    (args : Std.Array core.fmt.rt.Argument M) : Result core.fmt.Arguments :=
  .ok ⟨bytes.val, args.val⟩
-- The external panic runtime is still a modeled observation, not an IC trap proof.
def core.panicking.panic_fmt (_ : core.fmt.Arguments) : Result Never := .fail .panic
end LedgerPanicPayload
