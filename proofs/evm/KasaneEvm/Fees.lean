import Std

namespace KasaneEvm

def max64 : Nat := 2^64 - 1
def max128 : Nat := 2^128 - 1
def max256 : Nat := 2^256 - 1
def satAdd (limit a b : Nat) : Nat := min limit (a + b)

theorem sat_add_bounded (limit a b : Nat) : satAdd limit a b ≤ limit := by
  exact Nat.min_le_left _ _

theorem sat_add_exact_iff (limit a b : Nat) :
    satAdd limit a b = a + b ↔ a + b ≤ limit := by
  unfold satAdd
  omega

theorem sat_add_associative (limit a b c : Nat) :
    satAdd limit (satAdd limit a b) c = satAdd limit a (satAdd limit b c) := by
  unfold satAdd
  omega

theorem sat_add_monotone (limit a b c d : Nat) (ha : a ≤ c) (hb : b ≤ d) :
    satAdd limit a b ≤ satAdd limit c d := by
  unfold satAdd
  omega

/-- Translation of verified_core::fee::effective_gas_price. Domain bounds
are hypotheses of theorems, including the Rust u128/u64 conversion bounds. -/
def effectivePrice (cap priority base : Nat) : Option Nat :=
  if priority > cap ∨ cap < base then none
  else
    let price := min cap (satAdd max128 base priority)
    if price ≤ max64 then some price else none

theorem effective_price_exact (cap priority base price : Nat)
    (hcap : cap ≤ max128) (h : effectivePrice cap priority base = some price) :
    priority ≤ cap ∧ base ≤ price ∧ price ≤ cap ∧ price ≤ max64 ∧
      price = min cap (base + priority) := by
  unfold effectivePrice satAdd at h
  split at h
  · simp at h
  · dsimp only at h
    split at h
    · simp only [Option.some.injEq] at h
      omega
    · simp at h

theorem effective_price_rejects (cap priority base : Nat)
    (h : priority > cap ∨ cap < base ∨ min cap (satAdd max128 base priority) > max64) :
    effectivePrice cap priority base = none := by
  unfold effectivePrice
  split
  · rfl
  · dsimp only
    split <;> simp_all <;> omega

theorem effective_price_accepts (cap priority base : Nat)
    (hc : cap ≤ max128) (hp : priority ≤ cap) (hb : base ≤ cap)
    (hf : min cap (base + priority) ≤ max64) :
    effectivePrice cap priority base = some (min cap (base + priority)) := by
  have he : min cap (satAdd max128 base priority) = min cap (base + priority) := by
    unfold satAdd
    omega
  simp only [effectivePrice]
  rw [if_neg (by omega)]
  rw [he, if_pos hf]

theorem effective_price_some_iff (cap priority base price : Nat) (hc : cap ≤ max128) :
    effectivePrice cap priority base = some price ↔
      priority ≤ cap ∧ base ≤ cap ∧ min cap (base + priority) ≤ max64 ∧
        price = min cap (base + priority) := by
  constructor
  · intro h
    have bounds := effective_price_exact cap priority base price hc h
    exact ⟨bounds.1, Nat.le_trans bounds.2.1 bounds.2.2.1,
      by rw [← bounds.2.2.2.2]; exact bounds.2.2.2.1, bounds.2.2.2.2⟩
  · rintro ⟨hp, hb, hf, rfl⟩
    exact effective_price_accepts cap priority base hc hp hb hf

theorem effective_price_none_iff (cap priority base : Nat) (hc : cap ≤ max128) :
    effectivePrice cap priority base = none ↔
      cap < priority ∨ cap < base ∨ max64 < min cap (base + priority) := by
  have he : min cap (satAdd max128 base priority) = min cap (base + priority) := by
    unfold satAdd
    omega
  simp only [effectivePrice]
  rw [he]
  by_cases h : priority > cap ∨ cap < base
  · simp [h]
    omega
  · simp [h]
    omega

/-- The transaction adapter maps absent priority (legacy/EIP-2930) to cap. -/
def transactionPrice (cap : Nat) (priority : Option Nat) (base : Nat) : Option Nat :=
  effectivePrice cap (priority.getD cap) base

theorem legacy_price_exact (cap base : Nat) (hb : base ≤ cap) (hc : cap ≤ max64) :
    transactionPrice cap none base = some cap := by
  have h128 : cap ≤ max128 := by
    have bounds : max64 ≤ max128 := by decide
    omega
  have hm : min cap (base + cap) = cap := by omega
  simpa [transactionPrice, hm] using
    effective_price_accepts cap cap base h128 (Nat.le_refl _) hb (by omega)

theorem u64_product_fits_u128 (gas price : Nat)
    (hg : gas ≤ max64) (hp : price ≤ max64) : gas * price ≤ max128 := by
  have h := Nat.mul_le_mul hg hp
  have bound : max64 * max64 ≤ max128 := by decide
  exact Nat.le_trans h bound

def totalFee (gas price l1 operator : Nat) : Nat :=
  satAdd max128 (satAdd max128 (gas * price) l1) operator

theorem execution_fee_exact (gas price : Nat)
    (hg : gas ≤ max64) (hp : price ≤ max64) :
    totalFee gas price 0 0 = gas * price := by
  have h := u64_product_fits_u128 gas price hg hp
  simp [totalFee, satAdd, Nat.min_eq_right h]

theorem total_fee_saturates_once (gas price l1 operator : Nat) :
    totalFee gas price l1 operator = min max128 (gas * price + l1 + operator) := by
  unfold totalFee satAdd
  omega

theorem total_fee_bounded (gas price l1 operator : Nat) :
    totalFee gas price l1 operator ≤ max128 := by
  exact sat_add_bounded _ _ _

theorem total_fee_exact_iff (gas price l1 operator : Nat) :
    totalFee gas price l1 operator = gas * price + l1 + operator ↔
      gas * price + l1 + operator ≤ max128 := by
  rw [total_fee_saturates_once]
  omega

theorem total_fee_covers_execution (gas price l1 operator : Nat)
    (hg : gas ≤ max64) (hp : price ≤ max64) :
    gas * price ≤ totalFee gas price l1 operator := by
  have h := u64_product_fits_u128 gas price hg hp
  rw [total_fee_saturates_once]
  omega

theorem total_fee_monotone (gas price l1 operator gas' price' l1' operator' : Nat)
    (hg : gas ≤ gas') (hp : price ≤ price') (hl : l1 ≤ l1') (ho : operator ≤ operator') :
    totalFee gas price l1 operator ≤ totalFee gas' price' l1' operator' := by
  have hm := Nat.mul_le_mul hg hp
  rw [total_fee_saturates_once, total_fee_saturates_once]
  omega

theorem credit_preserves_balance_without_overflow (balance reward : Nat)
    (h : balance + reward ≤ max256) :
    satAdd max256 balance reward = balance + reward := by
  exact Nat.min_eq_right h

theorem credit_saturates_on_overflow (balance reward : Nat)
    (h : max256 ≤ balance + reward) : satAdd max256 balance reward = max256 := by
  exact Nat.min_eq_left h

/-- Existing account information (from the diff or the DB) is supplied by the
adapter. Only the balance and touch flag change during the base-fee credit. -/
structure AccountInfo where
  balance : Nat
  nonce : Nat
  codeHash : Nat
  touched : Bool
  deriving DecidableEq, Repr

def creditBaseFee (account : AccountInfo) (gas base : Nat) : AccountInfo :=
  if gas = 0 ∨ base = 0 then account
  else { account with balance := satAdd max256 account.balance (gas * base), touched := true }

theorem credit_preserves_metadata (account : AccountInfo) (gas base : Nat) :
    (creditBaseFee account gas base).nonce = account.nonce ∧
    (creditBaseFee account gas base).codeHash = account.codeHash := by
  unfold creditBaseFee
  split <;> exact ⟨rfl, rfl⟩

theorem credit_balance_exact (account : AccountInfo) (gas base : Nat)
    (h : account.balance + gas * base ≤ max256) :
    (creditBaseFee account gas base).balance = account.balance + gas * base := by
  unfold creditBaseFee
  split
  · rename_i hz
    rcases hz with hg | hb
    · simp [hg]
    · simp [hb]
  · exact credit_preserves_balance_without_overflow _ _ h

theorem credit_balance_bounded (account : AccountInfo) (gas base : Nat)
    (h : account.balance ≤ max256) : (creditBaseFee account gas base).balance ≤ max256 := by
  unfold creditBaseFee
  split
  · exact h
  · exact sat_add_bounded _ _ _

theorem credit_balance_nondecreasing (account : AccountInfo) (gas base : Nat)
    (h : account.balance ≤ max256) : account.balance ≤ (creditBaseFee account gas base).balance := by
  unfold creditBaseFee
  split
  · exact Nat.le_refl _
  · simp only [satAdd]
    omega

theorem positive_credit_touches (account : AccountInfo) (gas base : Nat)
    (hg : gas ≠ 0) (hb : base ≠ 0) : (creditBaseFee account gas base).touched = true := by
  simp [creditBaseFee, hg, hb]

theorem credit_preserves_existing_touch (account : AccountInfo) (gas base : Nat)
    (h : account.touched = true) : (creditBaseFee account gas base).touched = true := by
  unfold creditBaseFee
  split <;> simp_all

/-- The base-fee portion cannot exceed the execution fee of an accepted price. -/
theorem base_reward_le_execution_fee (cap priority base price gas : Nat)
    (hc : cap ≤ max128) (h : effectivePrice cap priority base = some price) :
    gas * base ≤ gas * price := by
  exact Nat.mul_le_mul_left gas (effective_price_exact _ _ _ _ hc h).2.1

end KasaneEvm
