import StdCellGetGenerated
namespace StdCellGetState

theorem actual_effect_chain (api : API) (p : Pointer) (h : Heap) :
    execute api actualCellGet (initial p) h =
      bindPointer (api.project p h) (fun p h =>
        bindPointer (api.unsafeGet p h) (fun p h => api.read p h)) := by
  cases first : api.project p h with
  | fault reason heap => simp [execute, actualCellGet, initial, put, bindPointer, first]
  | returned pointer heap =>
    cases second : api.unsafeGet pointer heap with
    | fault reason heap => simp [execute, actualCellGet, initial, put, bindPointer, first, second]
    | returned pointer2 heap2 =>
      cases third : api.read pointer2 heap2 <;>
        simp [execute, actualCellGet, initial, put, bindPointer, first, second, third]

-- Composition helper retains failure heaps; no effect identity premise.
theorem bind_pointer_chain (result : PointerOutcome)
    (next : Pointer → Heap → PointerOutcome) (last : Pointer → Heap → Outcome) :
    bindPointer (StdUnsafeCellPointer.bind result next) last =
      bindPointer result (fun p h => bindPointer (next p h) last) := by
  cases result <;> rfl

theorem linked_five_effects (project : Pointer → Heap → PointerOutcome)
    (pointerAPI : StdUnsafeCellPointer.API) (rights : Permissions) (p : Pointer) (h : Heap) :
    execute (linkedAPI project pointerAPI rights) actualCellGet (initial p) h =
      bindPointer (project p h) (fun p h =>
        bindPointer (pointerAPI.address p h) (fun p h =>
          bindPointer (pointerAPI.cast .cellToScalar p h) (fun p h =>
            bindPointer (pointerAPI.cast .constToMutable p h) (fun p h => readHeap rights p h)))) := by
  simp only [actual_effect_chain, linkedAPI, StdUnsafeCellPointer.actual_effect_chain,
    bind_pointer_chain]

theorem linked_read_conditional (project : Pointer → Heap → PointerOutcome)
    (projectionIdentity : ∀ p h, project p h = .returned p h)
    (pointerAPI : StdUnsafeCellPointer.API) (contracts : StdUnsafeCellPointer.IdentityContracts pointerAPI)
    (rights : Permissions) (p : Pointer) (h : Heap) (value : Scalar)
    (readable : rights.read p = true) (initialized : h p.address = some value) :
    execute (linkedAPI project pointerAPI rights) actualCellGet (initial p) h =
      .returned value h := by
  rw [actual_effect_chain]
  simp [linkedAPI, projectionIdentity, StdUnsafeCellPointer.actual_identity_conditional
    pointerAPI contracts, bindPointer, readHeap, readable, initialized]

theorem linked_denied_read_conditional (project : Pointer → Heap → PointerOutcome)
    (projectionIdentity : ∀ p h, project p h = .returned p h)
    (pointerAPI : StdUnsafeCellPointer.API) (contracts : StdUnsafeCellPointer.IdentityContracts pointerAPI)
    (rights : Permissions) (p : Pointer) (h : Heap) (denied : rights.read p = false) :
    execute (linkedAPI project pointerAPI rights) actualCellGet (initial p) h =
      .fault .deniedRead h := by
  rw [actual_effect_chain]
  simp [linkedAPI, projectionIdentity, StdUnsafeCellPointer.actual_identity_conditional
    pointerAPI contracts, bindPointer, readHeap, denied]

theorem projection_failure_preserved (api : API) (p : Pointer) (h changed : Heap)
    (reason : StdUnsafeCellPointer.Fault) (failure : api.project p h = .fault reason changed) :
    execute api actualCellGet (initial p) h = .fault (.pointerFailure reason) changed := by
  rw [actual_effect_chain, failure]
  rfl

end StdCellGetState
