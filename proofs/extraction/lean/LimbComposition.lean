import WordCorrespondence
open Aeneas Aeneas.Std
namespace LimbComposition
open WordCorrespondence

def value : List U64 → Nat
  | [] => 0
  | a :: rest => a.val + radix * value rest

def add : List U64 → List U64 → Bool → Result (List U64 × Bool)
  | [], [], carry => .ok ([], carry)
  | a :: ar, b :: br, carry => do
      let (lo, next) ← WordExtracted.ruint.algorithms.add.carrying_add a b carry
      let (rest, out) ← add ar br next
      .ok (lo :: rest, out)
  | _, _, _ => .fail .panic

def sub : List U64 → List U64 → Bool → Result (List U64 × Bool)
  | [], [], borrow => .ok ([], borrow)
  | a :: ar, b :: br, borrow => do
      let (lo, next) ← WordExtracted.ruint.algorithms.add.borrowing_sub a b borrow
      let (rest, out) ← sub ar br next
      .ok (lo :: rest, out)
  | _, _, _ => .fail .panic

theorem value_bound (limbs : List U64) : value limbs < radix ^ limbs.length := by
  induction limbs with
  | nil => simp [value]
  | cons a rest ih =>
    have ha := a.hBounds
    simp only [UScalarTy.numBits] at ha
    simp only [value, List.length_cons, pow_succ]
    unfold radix at *
    nlinarith

theorem add_balance_all_lengths (a b : List U64) (same : a.length = b.length)
    (carry : Bool) :
    ∃ result : List U64 × Bool, add a b carry = .ok result ∧
      result.1.length = a.length ∧
      value result.1 + radix ^ a.length * result.2.toNat =
        value a + value b + carry.toNat := by
  induction a generalizing b carry with
  | nil => cases b <;> simp_all [add, value]
  | cons a ar ih =>
    cases b with
    | nil => simp at same
    | cons b br =>
      have lengths : ar.length = br.length := by simpa using same
      obtain ⟨head, hh, hb⟩ := carrying_add_all_inputs a b carry
      obtain ⟨tail, ht, hl, htbal⟩ := ih br lengths head.2
      refine ⟨(head.1 :: tail.1, tail.2), ?_, ?_, ?_⟩
      · cases head; cases tail; simp [add, hh, ht]
      · simp [hl]
      · have headbal : head.1.val + radix * head.2.toNat =
            a.val + b.val + carry.toNat := by
          cases h : head.2 <;> simpa [h] using hb
        have scaled := congrArg (fun x : Nat => radix * x) htbal
        simp only [value, List.length_cons, pow_succ]
        nlinarith only [headbal, scaled]

theorem sub_balance_all_lengths (a b : List U64) (same : a.length = b.length)
    (borrow : Bool) :
    ∃ result : List U64 × Bool, sub a b borrow = .ok result ∧
      result.1.length = a.length ∧
      value a + radix ^ a.length * result.2.toNat =
        value result.1 + value b + borrow.toNat := by
  induction a generalizing b borrow with
  | nil => cases b <;> simp_all [sub, value]
  | cons a ar ih =>
    cases b with
    | nil => simp at same
    | cons b br =>
      have lengths : ar.length = br.length := by simpa using same
      obtain ⟨head, hh, hb⟩ := borrowing_sub_all_inputs a b borrow
      obtain ⟨tail, ht, hl, htbal⟩ := ih br lengths head.2
      refine ⟨(head.1 :: tail.1, tail.2), ?_, ?_, ?_⟩
      · cases head; cases tail; simp [sub, hh, ht]
      · simp [hl]
      · have headbal : a.val + radix * head.2.toNat =
            head.1.val + b.val + borrow.toNat := by
          cases h : head.2 <;> simpa [h] using hb
        have scaled := congrArg (fun x : Nat => radix * x) htbal
        simp only [value, List.length_cons, pow_succ]
        nlinarith only [headbal, scaled]

theorem add_modular_all_lengths (a b : List U64) (same : a.length = b.length) :
    ∃ result : List U64 × Bool, add a b false = .ok result ∧
      result.1.length = a.length ∧
      value result.1 = (value a + value b) % radix ^ a.length := by
  obtain ⟨result, hr, hl, balance⟩ := add_balance_all_lengths a b same false
  refine ⟨result, hr, hl, ?_⟩
  have bound := value_bound result.1
  rw [hl] at bound
  simp only [Bool.toNat_false, Nat.add_zero] at balance
  have hm := congrArg (fun x : Nat => x % radix ^ a.length) balance
  simpa [Nat.add_mod, Nat.mul_mod, Nat.mod_eq_of_lt bound] using hm

theorem sub_modular_all_lengths (a b : List U64) (same : a.length = b.length) :
    ∃ result : List U64 × Bool, sub a b false = .ok result ∧
      result.1.length = a.length ∧
      value result.1 = (value a + radix ^ a.length - value b) % radix ^ a.length := by
  obtain ⟨result, hr, hl, balance⟩ := sub_balance_all_lengths a b same false
  refine ⟨result, hr, hl, ?_⟩
  have bound := value_bound result.1
  rw [hl] at bound
  cases h : result.2
  · simp only [h, Bool.toNat_false, Nat.mul_zero, Nat.add_zero] at balance
    have hd : value a + radix ^ a.length - value b = value result.1 + radix ^ a.length := by omega
    simp [hd, Nat.mod_eq_of_lt bound]
  · simp only [h, Bool.toNat_true, Bool.toNat_false, Nat.mul_one, Nat.add_zero] at balance
    have hd : value a + radix ^ a.length - value b = value result.1 := by omega
    rw [hd, Nat.mod_eq_of_lt bound]

theorem four_limbs_radix : radix ^ 4 = 2^256 := by norm_num [radix]

end LimbComposition
