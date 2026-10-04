import KasaneEvm

open KasaneEvm

/-- Constants mirror the pinned Rust constants file; vectors check its boundary
values against the actual validator. Theorems quantify over arbitrary limits. -/
def limits : SizeLimits := ⟨32768, 64, 4, 4096⟩

def main : IO Unit := do
  for output in [0, limits.output - 1, limits.output, limits.output + 1] do
    for count in [0, limits.logs - 1, limits.logs, limits.logs + 1] do
      for topics in [0, limits.topics, limits.topics + 1] do
        for data in [0, limits.data, limits.data + 1] do
          let valid := resultSizesValid limits output (List.replicate count ⟨topics, data⟩)
          IO.println s!"size {output} {count} {topics} {data} {valid}"
  for (topics, data) in [(limits.topics + 1, 0), (0, limits.data + 1)] do
    let valid := resultSizesValid limits 0 [⟨0, 0⟩, ⟨topics, data⟩]
    IO.println s!"mixed {topics} {data} {valid}"
  for address in [1, 2] do
    for (label, scheme) in [("call", Authorization.Scheme.call), ("callCode", .callCode),
        ("delegateCall", .delegateCall), ("staticCall", .staticCall)] do
      for targetMatches in [false, true] do
        for bytecodeMatches in [false, true] do
          for transfers in [false, true] do
            for isStatic in [false, true] do
              for allowExternal in [false, true] do
                let context : Authorization.Context :=
                  ⟨scheme, if targetMatches then address else 0,
                    if bytecodeMatches then address else 0, transfers, isStatic, allowExternal⟩
                let allowed := Authorization.assetCallAllowed context address
                IO.println s!"asset {address} {label} {targetMatches} {bytecodeMatches} {transfers} {isStatic} {allowExternal} {allowed}"
