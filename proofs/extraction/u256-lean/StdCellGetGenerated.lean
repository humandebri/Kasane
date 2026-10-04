import StdCellGetState
-- Actual Cell::get main body; exact two-statement unwind cleanup audited.
-- Scalar IR fault propagation abstracts Rust unwinding; physical refinement unproved.
namespace StdCellGetState
def actualCellGet : List Instruction := [
  .live 0,
  .live 2,
  .live 3,
  .project 3 1,
  .unsafeGet 2 3,
  .dead 3,
  .read 0 2,
  .dead 2,
  .dead 1,
  .ret
]
end StdCellGetState
