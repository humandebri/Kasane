import Std

namespace KasaneEvm.Authorization

/-- Projection of the four schemes in the pinned revm CallInputs type. -/
inductive Scheme where
  | call | callCode | delegateCall | staticCall
  deriving DecidableEq, Repr

structure Context where
  scheme : Scheme
  target : Nat
  bytecode : Nat
  transfersValue : Bool
  isStatic : Bool
  allowExternal : Bool
  deriving Repr

/-- Admission checks before parsing, burning tokens or emitting an intent log.
Addresses are opaque identities; revm's construction of CallInputs, authority,
value conversion, ABI parsing and ledger dispatch are separate obligations. -/
def assetCallAllowed (context : Context) (address : Nat) : Bool :=
  decide (context.allowExternal = true ∧ context.isStatic = false ∧
    context.scheme = .call ∧ context.target = address ∧ context.bytecode = address ∧
    context.transfersValue = true)

theorem asset_call_allowed_iff (context : Context) (address : Nat) :
    assetCallAllowed context address = true ↔
      context.allowExternal = true ∧ context.isStatic = false ∧
      context.scheme = .call ∧ context.target = address ∧ context.bytecode = address ∧
      context.transfersValue = true := by simp [assetCallAllowed]

theorem non_call_scheme_rejected (context : Context) (address : Nat)
    (h : context.scheme ≠ .call) : assetCallAllowed context address = false := by
  simp [assetCallAllowed, h]

theorem static_context_rejected (context : Context) (address : Nat)
    (h : context.isStatic = true) : assetCallAllowed context address = false := by
  simp [assetCallAllowed, h]

theorem external_disabled_rejected (context : Context) (address : Nat)
    (h : context.allowExternal = false) : assetCallAllowed context address = false := by
  simp [assetCallAllowed, h]

theorem apparent_value_rejected (context : Context) (address : Nat)
    (h : context.transfersValue = false) : assetCallAllowed context address = false := by
  simp [assetCallAllowed, h]

theorem wrong_target_rejected (context : Context) (address : Nat)
    (h : context.target ≠ address) : assetCallAllowed context address = false := by
  simp [assetCallAllowed, h]

theorem wrong_bytecode_rejected (context : Context) (address : Nat)
    (h : context.bytecode ≠ address) : assetCallAllowed context address = false := by
  simp [assetCallAllowed, h]

/-- Matching bytecode alone cannot grant another target's asset authority. -/
theorem accepted_call_has_matching_addresses (context : Context) (address : Nat)
    (h : assetCallAllowed context address = true) : context.target = context.bytecode := by
  have bounds := (asset_call_allowed_iff context address).mp h
  exact bounds.2.2.2.1.trans bounds.2.2.2.2.1.symm

theorem direct_call_admitted (address : Nat) :
    assetCallAllowed ⟨.call, address, address, true, false, true⟩ address = true := by
  simp [assetCallAllowed]

end KasaneEvm.Authorization
