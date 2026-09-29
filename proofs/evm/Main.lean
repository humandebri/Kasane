import KasaneEvm

open KasaneEvm

def main : IO Unit := do
  let values := [0, 1, 7, max64 - 1, max64, max64 + 1, max128 - 1, max128]
  for cap in values do
    for priority in values do
      for base in [0, 1, 7, max64] do
        let result := match effectivePrice cap priority base with
          | none => "none"
          | some price => toString price
        IO.println s!"price {cap} {priority} {base} {result}"
  for gas in [0, 1, 21000, max64] do
    for price in [0, 1, 7, max64] do
      for l1 in [0, 1, max128] do
        for operator in [0, 1, max128] do
          IO.println s!"fee {gas} {price} {l1} {operator} {totalFee gas price l1 operator}"
      IO.println s!"reward {gas} {price} {gas * price}"
  for destroyed in [false, true] do
    for empty in [false, true] do
      for touched in [false, true] do
        IO.println s!"account {destroyed} {empty} {touched} {deleteAccount destroyed empty touched}"
  for hasCode in [false, true] do
    for empty in [false, true] do
      let decision := match codeDecision hasCode empty with
        | .skip => "skip"
        | .remove => "remove"
        | .insert => "insert"
      IO.println s!"code {hasCode} {empty} {decision}"
  for value in [0, 1, max256] do
    let decision := if (storageValue value).isNone then "remove" else "insert"
    IO.println s!"storage {value} {decision}"
