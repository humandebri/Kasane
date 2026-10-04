import StdMemReplaceGenerated
namespace StdMemReplaceState

-- All initialized scalar heaps and explicit permission inputs; conditional IR
-- correctness only, not an all-input theorem about actual Rust memory execution.
theorem actual_replace_scalar_heap (rights : Permissions) (h : Heap)
    (p : Pointer) (old newValue : Scalar)
    (hr : rights.read p = true) (hw : rights.write p = true)
    (initialized : h p.address = some old) :
    execute rights actualMemReplace (initial p newValue) h =
      .returned old (writeHeap h p.address newValue) := by
  simp [execute, actualMemReplace, initial, put, hr, hw, initialized]

theorem denied_read_preserves_heap (rights : Permissions) (h : Heap)
    (p : Pointer) (newValue : Scalar) (denied : rights.read p = false) :
    execute rights actualMemReplace (initial p newValue) h = .fault .deniedRead h := by
  simp [execute, actualMemReplace, initial, put, denied]

theorem denied_write_preserves_heap (rights : Permissions) (h : Heap)
    (p : Pointer) (old newValue : Scalar) (hr : rights.read p = true)
    (denied : rights.write p = false) (initialized : h p.address = some old) :
    execute rights actualMemReplace (initial p newValue) h = .fault .deniedWrite h := by
  simp [execute, actualMemReplace, initial, put, hr, denied, initialized]

theorem uninitialized_preserves_heap (rights : Permissions) (h : Heap)
    (p : Pointer) (newValue : Scalar) (hr : rights.read p = true)
    (uninitialized : h p.address = none) :
    execute rights actualMemReplace (initial p newValue) h = .fault .uninitialized h := by
  simp [execute, actualMemReplace, initial, put, hr, uninitialized]

theorem alias_observes_write (h : Heap) (p otherAlias : Pointer) (newValue : Scalar)
    (sameAddress : otherAlias.address = p.address) :
    writeHeap h p.address newValue otherAlias.address = some newValue := by
  simp [writeHeap, sameAddress]

theorem other_address_preserved (h : Heap) (p other : Pointer) (newValue : Scalar)
    (differentAddress : other.address ≠ p.address) :
    writeHeap h p.address newValue other.address = h other.address := by
  simp [writeHeap, differentAddress]

theorem actual_replace_alias_and_frame (rights : Permissions) (h : Heap)
    (p otherAlias : Pointer) (old newValue : Scalar)
    (hr : rights.read p = true) (hw : rights.write p = true)
    (initialized : h p.address = some old) (sameAddress : otherAlias.address = p.address) :
    ∃ updated : Heap,
      execute rights actualMemReplace (initial p newValue) h = .returned old updated ∧
      updated otherAlias.address = some newValue ∧
      ∀ a, a ≠ p.address → updated a = h a := by
  refine ⟨writeHeap h p.address newValue,
    actual_replace_scalar_heap rights h p old newValue hr hw initialized, ?_, ?_⟩
  · exact alias_observes_write h p otherAlias newValue sameAddress
  · intro a different
    simp [writeHeap, different]

end StdMemReplaceState
