import StdUnsafeCellGenerated
namespace StdUnsafeCellPointer

-- Actual extracted operation order; no identity/success assumption for effects.
theorem actual_effect_chain (api : API) (p : Pointer) (h : Heap) :
    execute api actualUnsafeCellGet (initial p) h =
      bind (api.address p h) (fun p h =>
        bind (api.cast .cellToScalar p h) (fun p h =>
          api.cast .constToMutable p h)) := by
  cases first : api.address p h with
  | fault reason heap => simp [execute, actualUnsafeCellGet, initial, put, bind, first]
  | returned pointer heap =>
    cases second : api.cast .cellToScalar pointer heap with
    | fault reason heap => simp [execute, actualUnsafeCellGet, initial, put, bind, first, second]
    | returned pointer2 heap2 =>
      cases third : api.cast .constToMutable pointer2 heap2 <;>
        simp [execute, actualUnsafeCellGet, initial, put, bind, first, second, third]

-- These contracts are premises, not established facts about physical Rust casts.
theorem actual_identity_conditional (api : API) (contracts : IdentityContracts api)
    (p : Pointer) (h : Heap) :
    execute api actualUnsafeCellGet (initial p) h = .returned p h := by
  rw [actual_effect_chain, contracts.address]
  simp only [bind, contracts.cast]

theorem identity_model (p : Pointer) (h : Heap) :
    execute identityAPI actualUnsafeCellGet (initial p) h = .returned p h := by
  apply actual_identity_conditional identityAPI
  exact ⟨fun _ _ => rfl, fun _ _ _ => rfl⟩

theorem conditional_alias_observation (api : API) (contracts : IdentityContracts api)
    (p otherAlias : Pointer) (h : Heap) (sameAddress : otherAlias.address = p.address) :
    ∃ output : Pointer, ∃ updated : Heap,
      execute api actualUnsafeCellGet (initial p) h = .returned output updated ∧
      output.address = otherAlias.address ∧ output.tag = p.tag ∧ updated = h := by
  exact ⟨p, h, actual_identity_conditional api contracts p h,
    sameAddress.symm, rfl, rfl⟩

end StdUnsafeCellPointer
