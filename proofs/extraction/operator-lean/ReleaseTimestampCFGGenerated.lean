import ReleaseTimestampCFG
namespace ReleaseTimestampCFG
def releaseAddCFG : Graph
  | 0 => some ⟨[.input 4, .asNanos 7], some 1⟩
  | 1 => some ⟨[.narrow 6 7], some 2⟩
  | 2 => some ⟨[.unwrap 5 6], some 3⟩
  | 3 => some ⟨[.saturate .add 3 4 5], some 4⟩
  | 4 => some ⟨[.returnTimestamp 3], none⟩
  | _ => none
def releaseSubCFG : Graph
  | 0 => some ⟨[.input 4, .asNanos 7], some 1⟩
  | 1 => some ⟨[.narrow 6 7], some 2⟩
  | 2 => some ⟨[.unwrap 5 6], some 3⟩
  | 3 => some ⟨[.saturate .sub 3 4 5], some 4⟩
  | 4 => some ⟨[.returnTimestamp 3], none⟩
  | _ => none
end ReleaseTimestampCFG
