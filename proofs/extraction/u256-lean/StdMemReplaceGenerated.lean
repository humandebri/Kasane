import StdMemReplaceState
-- Generated from the complete fixed actual mem::replace LLBC body.
-- Scalar-instance heap IR only; Rust pointer/retag correspondence is unproved.
namespace StdMemReplaceState
def actualMemReplace : List Instruction := [
  .live 0,
  .live 3,
  .live 4,
  .address .shared 4 1,
  .read 3 4,
  .dead 4,
  .live 5,
  .address .mutable 5 1,
  .write 5 2,
  .dead 5,
  .move 0 3,
  .dead 3,
  .dead 2,
  .dead 1,
  .ret
]
end StdMemReplaceState
