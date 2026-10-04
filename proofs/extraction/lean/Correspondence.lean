import KasaneExtracted
import KasaneEvm.State
import KasaneEvm.Fees
open Aeneas Aeneas.Std
namespace Correspondence

def encodeAccount : KasaneEvm.AccountDecision → Extracted.state_diff.AccountCommitDecision
  | .skip => .Skip
  | .delete => .Delete
  | .upsert => .Upsert

def encodeCode : KasaneEvm.CodeDecision → Extracted.state_diff.CodeCommitDecision
  | .skip => .Skip
  | .remove => .Remove
  | .insert => .Insert

theorem account_decision_all_inputs (destroyed empty touched : Bool) :
    Extracted.state_diff.account_commit_decision destroyed empty touched =
      .ok (encodeAccount (KasaneEvm.accountDecision destroyed empty touched)) := by
  cases destroyed <;> cases empty <;> cases touched <;> rfl

theorem code_decision_all_inputs (hasCode empty : Bool) :
    Extracted.state_diff.code_commit_decision hasCode empty =
      .ok (encodeCode (KasaneEvm.codeDecision hasCode empty)) := by
  cases hasCode <;> cases empty <;> rfl

theorem storage_decision_all_inputs (zero : Bool) :
    Extracted.state_diff.storage_commit_decision zero =
      .ok (if zero then .Remove else .Insert) := by
  cases zero <;> rfl

theorem account_empty_all_inputs (nonce : U64) (balanceZero codeEmpty : Bool) :
    Extracted.state_diff.account_is_empty nonce balanceZero codeEmpty =
      .ok (KasaneEvm.accountIsEmpty ⟨nonce.val, if balanceZero then 0 else 1, codeEmpty⟩) := by
  cases balanceZero <;> cases codeEmpty <;>
    simp [Extracted.state_diff.account_is_empty, KasaneEvm.accountIsEmpty]
  by_cases h : nonce = 0#u64
  · simp [h]
  · have hn : nonce.val ≠ 0 := by
      intro hz
      apply h
      apply UScalar.eq_of_val_eq
      simpa using hz
    simp [h, hn]

theorem terminal_all_inputs (status : U64) :
    Extracted.unwrap_dispatch.unwrap_dispatch_terminal_raw status =
      .ok (decide (status.val = 2 ∨ status.val = 3)) := by
  simp [Extracted.unwrap_dispatch.unwrap_dispatch_terminal_raw,
    Extracted.unwrap_dispatch.UNWRAP_STATUS_DISPATCHED,
    Extracted.unwrap_dispatch.UNWRAP_STATUS_DISPATCH_FAILED,
    UScalar.eq_equiv]
  split <;> simp_all

theorem retry_all_inputs (previous next inserted cleared : U64) :
    Extracted.unwrap_dispatch.unwrap_retry_transition_safe_raw previous next inserted cleared =
      .ok (decide (previous.val = 3 ∧ next.val = 0 ∧ inserted.val = 1 ∧ cleared.val = 1)) := by
  simp [Extracted.unwrap_dispatch.unwrap_retry_transition_safe_raw,
    Extracted.unwrap_dispatch.UNWRAP_STATUS_DISPATCH_FAILED,
    Extracted.unwrap_dispatch.UNWRAP_STATUS_QUEUED, UScalar.eq_equiv]
  split <;> simp_all
  split <;> simp_all
  split <;> simp_all

theorem l2_fee_all_inputs (gas price : U64) :
    ∃ fee, Extracted.fee.l2_fee gas price = .ok fee ∧ fee.val = gas.val * price.val := by
  have hb : gas.val * price.val ≤ U128.max := by
    simpa only [U128.max_eq, KasaneEvm.max128] using
      KasaneEvm.u64_product_fits_u128 gas.val price.val (U64.le_max gas) (U64.le_max price)
  have hp : (core.convert.num.FromU128U64.from gas).val *
      (core.convert.num.FromU128U64.from price).val ≤ U128.max := by simpa using hb
  have hm := U128.mul_spec (x := core.convert.num.FromU128U64.from gas)
    (y := core.convert.num.FromU128U64.from price) hp
  simp only [core.convert.num.FromU128U64.from_val_eq] at hm
  have he : Extracted.fee.l2_fee gas price =
      core.convert.num.FromU128U64.from gas * core.convert.num.FromU128U64.from price := by
    simp [Extracted.fee.l2_fee, lift]
  rw [he]
  exact WP.spec_imp_exists hm

theorem reward_all_inputs (gas base : U64) :
    ∃ fee, Extracted.fee.base_fee_reward gas base = .ok fee ∧ fee.val = gas.val * base.val := by
  exact l2_fee_all_inputs gas base

theorem saturating_add_value (a b : U128) :
    (core.num.U128.saturating_add a b).val =
      KasaneEvm.satAdd KasaneEvm.max128 a.val b.val := by
  change (min (UScalar.max .U128) (a.val + b.val)) % 2^128 =
    min KasaneEvm.max128 (a.val + b.val)
  rw [UScalar.max_UScalarTy_U128_eq, U128.max_eq]
  have hm : min 340282366920938463463374607431768211455 (a.val + b.val) ≤
      340282366920938463463374607431768211455 := Nat.min_le_left _ _
  have hb : min 340282366920938463463374607431768211455 (a.val + b.val) < 2^128 := by omega
  rw [Nat.mod_eq_of_lt hb]
  rfl

theorem total_fee_all_inputs (gas price : U64) (l1 operator : U128) :
    ∃ fee, Extracted.fee.total_fee gas price l1 operator = .ok fee ∧
      fee.val = KasaneEvm.totalFee gas.val price.val l1.val operator.val := by
  obtain ⟨execution, he, hv⟩ := l2_fee_all_inputs gas price
  refine ⟨core.num.U128.saturating_add (core.num.U128.saturating_add execution l1) operator, ?_, ?_⟩
  · simp [Extracted.fee.total_fee, he, lift]
  · simp [saturating_add_value, hv, KasaneEvm.totalFee]

theorem min_fee_all_inputs (gas : U128) (priority : Option U128)
    (base minPriority minGas : U64) :
    Extracted.fee.min_fee_satisfied gas priority base minPriority minGas =
      .ok (match priority with
        | none => decide (minGas.val ≤ gas.val)
        | some p => decide (minPriority.val ≤ p.val ∧ base.val ≤ gas.val ∧
            KasaneEvm.satAdd KasaneEvm.max128 base.val minPriority.val ≤ gas.val)) := by
  cases priority with
  | none =>
    simp [Extracted.fee.min_fee_satisfied, lift,
      UScalar.le_equiv, core.convert.num.FromU128U64.from_val_eq]
  | some p =>
    simp [Extracted.fee.min_fee_satisfied, lift, UScalar.lt_equiv, UScalar.le_equiv,
      saturating_add_value, core.convert.num.FromU128U64.from_val_eq]
    by_cases h : p.val < minPriority.val
    · simp [h]
    · simp [h]
      split <;> simp_all

set_option maxHeartbeats 4000000 in
theorem dispatch_all_inputs (previous next ledger error queued : U64) :
    Extracted.unwrap_dispatch.unwrap_dispatch_transition_safe_raw previous next ledger error queued =
      .ok (decide (
        (previous.val = 0 ∧ next.val = 1 ∧ ledger.val = 0 ∧ error.val = 0 ∧ queued.val = 0) ∨
        (previous.val = 1 ∧ next.val = 2 ∧ ledger.val = 1 ∧ error.val = 0 ∧ queued.val = 0) ∨
        (previous.val = 1 ∧ next.val = 3 ∧ ledger.val = 0 ∧ error.val = 1 ∧ queued.val = 0) ∨
        (previous.val = 3 ∧ next.val = 0 ∧ ledger.val = 0 ∧ error.val = 0 ∧ queued.val = 1))) := by
  simp (config := { maxSteps := 1000000 }) [Extracted.unwrap_dispatch.unwrap_dispatch_transition_safe_raw,
    Extracted.unwrap_dispatch.UNWRAP_STATUS_DISPATCH_FAILED,
    Extracted.unwrap_dispatch.UNWRAP_STATUS_DISPATCHED,
    Extracted.unwrap_dispatch.UNWRAP_STATUS_DISPATCHING,
    Extracted.unwrap_dispatch.UNWRAP_STATUS_QUEUED, UScalar.eq_equiv]
  by_cases h0 : previous.val = 0 <;>
    by_cases h1 : previous.val = 1 <;>
    by_cases h3 : previous.val = 3 <;>
    simp_all (config := { maxSteps := 1000000 }) <;>
    split_ifs <;> simp_all

set_option maxHeartbeats 4000000 in
theorem recovery_all_inputs (previous next already inserted updated : U64) :
    Extracted.unwrap_dispatch.unwrap_upgrade_recovery_safe_raw previous next already inserted updated =
      .ok (decide (
        (previous.val = 0 ∧ next.val = 0 ∧ updated.val = 0 ∧
          ((already.val = 1 ∧ inserted.val = 0) ∨ (already.val = 0 ∧ inserted.val = 1))) ∨
        (previous.val = 1 ∧ next.val = 0 ∧ already.val = 0 ∧ inserted.val = 1 ∧ updated.val = 1) ∨
        ((previous.val = 2 ∨ previous.val = 3) ∧ next.val = previous.val ∧
          inserted.val = 0 ∧ updated.val = 0))) := by
  simp (config := { maxSteps := 1000000 }) [Extracted.unwrap_dispatch.unwrap_upgrade_recovery_safe_raw,
    Extracted.unwrap_dispatch.UNWRAP_STATUS_DISPATCH_FAILED,
    Extracted.unwrap_dispatch.UNWRAP_STATUS_DISPATCHED,
    Extracted.unwrap_dispatch.UNWRAP_STATUS_DISPATCHING,
    Extracted.unwrap_dispatch.UNWRAP_STATUS_QUEUED, UScalar.eq_equiv]
  by_cases h0 : previous.val = 0 <;>
    by_cases h1 : previous.val = 1 <;>
    by_cases h2 : previous.val = 2 <;>
    by_cases h3 : previous.val = 3 <;>
    simp_all (config := { maxSteps := 1000000 }) <;>
    split_ifs <;> simp_all

theorem effective_price_all_inputs (cap priority : U128) (base : U64) :
    ∃ price, Extracted.fee.effective_gas_price cap priority base = .ok price ∧
      price.map UScalar.val = KasaneEvm.effectivePrice cap.val priority.val base.val := by
  let capped := core.cmp.impls.OrdU128.min cap
    (core.num.U128.saturating_add (core.convert.num.FromU128U64.from base) priority)
  have hc : capped.val = min cap.val
      (KasaneEvm.satAdd KasaneEvm.max128 base.val priority.val) := by
    simp [capped, saturating_add_value, core.convert.num.FromU128U64.from_val_eq]
  have hm : (core.convert.num.FromU128U64.from core.num.U64.MAX).val = KasaneEvm.max64 := by
    simp only [core.convert.num.FromU128U64.from_val_eq]
    change U64.rMax % 2^64 = 2^64 - 1
    simp [U64.rMax]
  have hm0 : core.num.U64.MAX.val = KasaneEvm.max64 := by simpa using hm
  by_cases hp : priority.val > cap.val
  · refine ⟨none, ?_, ?_⟩
    · simp [Extracted.fee.effective_gas_price, UScalar.lt_equiv, hp]
    · simp [KasaneEvm.effectivePrice, hp]
  · by_cases hb : cap.val < base.val
    · refine ⟨none, ?_, ?_⟩
      · simp [Extracted.fee.effective_gas_price, lift, UScalar.lt_equiv, hp, hb,
          core.convert.num.FromU128U64.from_val_eq]
      · simp [KasaneEvm.effectivePrice, hb]
    · by_cases limit : capped.val > KasaneEvm.max64
      all_goals have hl := limit
      all_goals rw [hc] at hl
      · refine ⟨none, ?_, ?_⟩
        · simp [Extracted.fee.effective_gas_price, lift, UScalar.lt_equiv,
            hp, hb, hm0, saturating_add_value, core.convert.num.FromU128U64.from_val_eq]
          omega
        · simp [KasaneEvm.effectivePrice, hp, hb, ← hc, limit]
      · refine ⟨some (UScalar.cast .U64 capped), ?_, ?_⟩
        · simp [Extracted.fee.effective_gas_price, lift, UScalar.lt_equiv,
            hp, hb, capped, hm0, saturating_add_value, core.convert.num.FromU128U64.from_val_eq]
          omega
        · simp [KasaneEvm.effectivePrice, hp, hb, UScalar.cast_val_eq,
            capped, saturating_add_value, core.convert.num.FromU128U64.from_val_eq]
          unfold KasaneEvm.max64 at *
          omega

end Correspondence
