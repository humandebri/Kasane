import KasaneEvm.Journal

open KasaneEvm.Journal

def observe (label : String) (s : World) (logs : List Nat) (depth : Nat) : IO Unit :=
  IO.println s!"{label} {s.storage 0} {s.storage 1} {s.source} {s.target} {logs.length} {depth} [{String.intercalate "," (logs.map toString)}]"

def main : IO Unit := do
  for old in [0, 7, 255] do
    for amount in [0, 1, 9] do
      let initial : World := ⟨fun k => if k = 0 then old else 0, 100, 20⟩
      let parent := run initial [.store 0 11, .transfer amount]
      IO.println s!"case {old} {amount}"
      let parentLogs := [1]
      observe "parent" parent.1 parentLogs 1
      let child := run parent.1 [.store 0 22, .store 1 33, .transfer 3]
      let childLogs := parentLogs ++ [2]
      observe "child" child.1 childLogs 2
      let restoredLogs := childLogs.take parentLogs.length
      observe "child_revert" (undo child.1 child.2) restoredLogs 1
      let retry := run parent.1 [.store 0 44, .transfer 2]
      let retryLogs := restoredLogs ++ [3]
      observe "child_commit" retry.1 retryLogs 1
      observe "parent_revert" (undo retry.1 (retry.2 ++ parent.2)) (retryLogs.take 0) 0
