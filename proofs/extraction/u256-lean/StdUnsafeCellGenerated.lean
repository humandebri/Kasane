import StdUnsafeCellPointer
-- Actual fixed UnsafeCell::get LLBC, scalar-instance pointer IR.
-- repr(transparent) metadata audited; physical cast/layout correspondence unproved.
namespace StdUnsafeCellPointer
def actualUnsafeCellGet : List Instruction := [
  .live 0,
  .live 2,
  .live 3,
  .live 4,
  .live 5,
  .live 6,
  .address 6 1,
  .copy 5 6,
  .cast .cellToScalar 4 5,
  .copy 3 4,
  .dead 5,
  .cast .constToMutable 2 3,
  .copy 0 2,
  .dead 6,
  .dead 4,
  .dead 3,
  .dead 2,
  .dead 1,
  .ret
]
end StdUnsafeCellPointer
