import LedgerPanicPayload.Funs
open Aeneas Aeneas.Std
namespace LedgerPanicPayloadCorrespondence
open LedgerPanicPayload

theorem try_from_success (u : U128) (fits : u.val ≤ UScalar.max .U64) :
    U64.Insts.CoreConvertTryFromU128TryFromIntError.try_from u =
      .ok (.Ok (UScalar.cast .U64 u)) := by
  simp [U64.Insts.CoreConvertTryFromU128TryFromIntError.try_from, lift,
    U64.rMax, U64.max, U64.numBits, UScalarTy.numBits] at fits ⊢
  exact fits

theorem try_from_overflow (u : U128) (over : UScalar.max .U64 < u.val) :
    U64.Insts.CoreConvertTryFromU128TryFromIntError.try_from u =
      .ok (.Err core.num.error.IntErrorKind.PosOverflow) := by
  simp [U64.Insts.CoreConvertTryFromU128TryFromIntError.try_from, lift,
    U64.rMax, U64.max, U64.numBits, UScalarTy.numBits] at over ⊢
  exact over

def durationValue (d : core.time.Duration) : Nat :=
  d.secs.val * 1000000000 + d.nanos.val.val

theorem durationValue_bound (d : core.time.Duration) : durationValue d < 2^128 := by
  have hs := d.secs.hBounds
  have hn := d.nanos.property.2
  simp only [UScalarTy.numBits] at hs
  unfold durationValue
  omega

theorem from_nanos_value (n : U64) :
    ∃ d, core.time.Duration.from_nanos n = .ok d ∧ durationValue d = n.val := by
  have hconst : UScalar.cast .U64 1000000000#u32 = 1000000000#u64 := by rfl
  obtain ⟨secs, hs, hsv⟩ := UScalar.div_spec n
    (y := 1000000000#u64) (by decide)
  obtain ⟨rem, hr, hrv⟩ := WP.spec_imp_exists
    (UScalar.rem_spec n (y := 1000000000#u64) (by decide))
  have hrem : rem.val < 1000000000 := by
    rw [hrv]
    exact Nat.mod_lt _ (by decide)
  have hcast : (UScalar.cast .U32 rem).val = rem.val := by
    apply UScalar.cast_val_mod_pow_of_inBounds_eq
    simp only [UScalarTy.numBits]
    omega
  have hvalid : (UScalar.cast .U32 rem).val < 1000000000 := by omega
  have upper : (UScalar.cast .U32 rem).val ≤ 999999999 := by omega
  have hnew : core.num.niche_types.Nanoseconds.new_unchecked (UScalar.cast .U32 rem) =
      .ok ⟨UScalar.cast .U32 rem, Nat.zero_le _, upper⟩ := by
    simp only [core.num.niche_types.Nanoseconds.new_unchecked, dif_pos upper]
  refine ⟨⟨secs, ⟨UScalar.cast .U32 rem, Nat.zero_le _, upper⟩⟩, ?_, ?_⟩
  · simp [core.time.Duration.from_nanos,
      core.time.Duration.from_nanos.NANOS_PER_SEC, core.time.NANOS_PER_SEC,
      hconst, hs, hr, lift, hnew]
  · change secs.val * 1000000000 + (UScalar.cast .U32 rem).val = n.val
    rw [hcast, hsv, hrv]
    change (n.val / 1000000000) * 1000000000 + n.val % 1000000000 = n.val
    omega

theorem as_nanos_value (d : core.time.Duration) :
    ∃ u, core.time.Duration.as_nanos d = .ok u ∧ u.val = durationValue d := by
  have hconst : UScalar.cast .U128 1000000000#u32 = 1000000000#u128 := by rfl
  have hsecs : (UScalar.cast .U128 d.secs).val = d.secs.val := by simp
  have hnanos : (UScalar.cast .U128 d.nanos.val).val = d.nanos.val.val := by simp
  have hproduct : (UScalar.cast .U128 d.secs).val * (1000000000#u128).val ≤
      UScalar.max .U128 := by
    rw [hsecs]
    have h := durationValue_bound d
    unfold durationValue at h
    simp only [UScalar.max, UScalarTy.numBits]
    change d.secs.val * 1000000000 ≤ 2^128 - 1
    omega
  obtain ⟨product, hp, hpv⟩ := WP.spec_imp_exists
    (UScalar.mul_spec hproduct)
  have hsum : product.val + (UScalar.cast .U128 d.nanos.val).val ≤
      UScalar.max .U128 := by
    rw [hpv, hsecs, hnanos]
    have h := durationValue_bound d
    unfold durationValue at h
    simp only [UScalar.max, UScalarTy.numBits]
    change d.secs.val * 1000000000 + d.nanos.val.val ≤ 2^128 - 1
    omega
  obtain ⟨sum, ha, hav⟩ := WP.spec_imp_exists (UScalar.add_spec hsum)
  refine ⟨sum, ?_, ?_⟩
  · simp [core.time.Duration.as_nanos, core.time.NANOS_PER_SEC,
      core.num.niche_types.Nanoseconds.as_inner, lift, hconst, hp, ha]
  · rw [hav, hpv, hsecs, hnanos]
    rfl

theorem add_success (t : timestamp.TimeStamp) (d : core.time.Duration)
    (fits : durationValue d ≤ UScalar.max .U64) :
    ∃ r, timestamp.TimeStamp.Insts.CoreOpsArithAddDurationTimeStamp.add t d = .ok r ∧
      r.timestamp_nanos.val = min (UScalar.max .U64)
        (t.timestamp_nanos.val + durationValue d) := by
  obtain ⟨u, hu, hv⟩ := as_nanos_value d
  have hc := try_from_success u (by simpa [hv] using fits)
  have hcv : (UScalar.cast .U64 u).val = durationValue d := by
    rw [UScalar.cast_val_mod_pow_of_inBounds_eq]
    · exact hv
    · simp only [UScalarTy.numBits]
      rw [hv]
      have h := fits
      simp only [UScalar.max, UScalarTy.numBits] at h
      omega
  refine ⟨⟨core.num.U64.saturating_add t.timestamp_nanos (UScalar.cast .U64 u)⟩, ?_, ?_⟩
  · simp [timestamp.TimeStamp.Insts.CoreOpsArithAddDurationTimeStamp.add,
      hu, hc, LedgerPanicPayload.core.result.Result.unwrap, lift]
  · change (UScalar.saturating_add _ _).val = _
    change (min (UScalar.max .U64) (t.timestamp_nanos.val +
      (UScalar.cast .U64 u).val)) % 2^64 = _
    rw [hcv]
    apply Nat.mod_eq_of_lt
    have h := Nat.min_le_left (UScalar.max .U64)
      (t.timestamp_nanos.val + durationValue d)
    simp only [UScalar.max, UScalarTy.numBits] at h ⊢
    omega

theorem sub_success (t : timestamp.TimeStamp) (d : core.time.Duration)
    (fits : durationValue d ≤ UScalar.max .U64) :
    ∃ r, timestamp.TimeStamp.Insts.CoreOpsArithSubDurationTimeStamp.sub t d = .ok r ∧
      r.timestamp_nanos.val = t.timestamp_nanos.val - durationValue d := by
  obtain ⟨u, hu, hv⟩ := as_nanos_value d
  have hc := try_from_success u (by simpa [hv] using fits)
  have hcv : (UScalar.cast .U64 u).val = durationValue d := by
    rw [UScalar.cast_val_mod_pow_of_inBounds_eq]
    · exact hv
    · simp only [UScalarTy.numBits]
      rw [hv]
      have h := fits
      simp only [UScalar.max, UScalarTy.numBits] at h
      omega
  refine ⟨⟨core.num.U64.saturating_sub t.timestamp_nanos (UScalar.cast .U64 u)⟩, ?_, ?_⟩
  · simp [timestamp.TimeStamp.Insts.CoreOpsArithSubDurationTimeStamp.sub,
      hu, hc, LedgerPanicPayload.core.result.Result.unwrap, lift]
  · change (UScalar.saturating_sub _ _).val = _
    change (max 0 (t.timestamp_nanos.val - (UScalar.cast .U64 u).val)) % 2^64 = _
    rw [hcv, max_eq_right (Nat.zero_le _)]
    apply Nat.mod_eq_of_lt
    have h := t.timestamp_nanos.hBounds
    simp only [UScalarTy.numBits] at h ⊢
    omega

theorem add_duration_overflow (t : timestamp.TimeStamp) (d : core.time.Duration)
    (over : UScalar.max .U64 < durationValue d) :
    timestamp.TimeStamp.Insts.CoreOpsArithAddDurationTimeStamp.add t d = .fail .panic := by
  obtain ⟨u, hu, hv⟩ := as_nanos_value d
  have hc := try_from_overflow u (by simpa [hv] using over)
  simp [timestamp.TimeStamp.Insts.CoreOpsArithAddDurationTimeStamp.add,
    hu, hc, LedgerPanicPayload.core.result.Result.unwrap, LedgerPanicPayload.core.result.unwrap_failed, LedgerPanicPayload.core.fmt.rt.Argument.new_display, LedgerPanicPayload.core.fmt.rt.Argument.new_debug, LedgerPanicPayload.core.fmt.Arguments.new, LedgerPanicPayload.core.panicking.panic_fmt]

theorem sub_duration_overflow (t : timestamp.TimeStamp) (d : core.time.Duration)
    (over : UScalar.max .U64 < durationValue d) :
    timestamp.TimeStamp.Insts.CoreOpsArithSubDurationTimeStamp.sub t d = .fail .panic := by
  obtain ⟨u, hu, hv⟩ := as_nanos_value d
  have hc := try_from_overflow u (by simpa [hv] using over)
  simp [timestamp.TimeStamp.Insts.CoreOpsArithSubDurationTimeStamp.sub,
    hu, hc, LedgerPanicPayload.core.result.Result.unwrap, LedgerPanicPayload.core.result.unwrap_failed, LedgerPanicPayload.core.fmt.rt.Argument.new_display, LedgerPanicPayload.core.fmt.rt.Argument.new_debug, LedgerPanicPayload.core.fmt.Arguments.new, LedgerPanicPayload.core.panicking.panic_fmt]

theorem add_success_iff (t : timestamp.TimeStamp) (d : core.time.Duration) :
    (∃ r, timestamp.TimeStamp.Insts.CoreOpsArithAddDurationTimeStamp.add t d = .ok r) ↔
      durationValue d ≤ UScalar.max .U64 := by
  constructor
  · intro h
    by_contra over
    have fail := add_duration_overflow t d (Nat.lt_of_not_ge over)
    obtain ⟨r, hr⟩ := h
    rw [fail] at hr
    exact fail_not_ok hr
  · intro fits
    obtain ⟨r, hr, _⟩ := add_success t d fits
    exact ⟨r, hr⟩

theorem sub_success_iff (t : timestamp.TimeStamp) (d : core.time.Duration) :
    (∃ r, timestamp.TimeStamp.Insts.CoreOpsArithSubDurationTimeStamp.sub t d = .ok r) ↔
      durationValue d ≤ UScalar.max .U64 := by
  constructor
  · intro h
    by_contra over
    have fail := sub_duration_overflow t d (Nat.lt_of_not_ge over)
    obtain ⟨r, hr⟩ := h
    rw [fail] at hr
    exact fail_not_ok hr
  · intro fits
    obtain ⟨r, hr, _⟩ := sub_success t d fits
    exact ⟨r, hr⟩

theorem add_panic_iff (t : timestamp.TimeStamp) (d : core.time.Duration) :
    timestamp.TimeStamp.Insts.CoreOpsArithAddDurationTimeStamp.add t d = .fail .panic ↔
      UScalar.max .U64 < durationValue d := by
  constructor
  · intro h
    by_contra fits
    obtain ⟨r, hr, _⟩ := add_success t d (Nat.le_of_not_gt fits)
    rw [h] at hr
    exact fail_not_ok hr
  · exact add_duration_overflow t d

theorem sub_panic_iff (t : timestamp.TimeStamp) (d : core.time.Duration) :
    timestamp.TimeStamp.Insts.CoreOpsArithSubDurationTimeStamp.sub t d = .fail .panic ↔
      UScalar.max .U64 < durationValue d := by
  constructor
  · intro h
    by_contra fits
    obtain ⟨r, hr, _⟩ := sub_success t d (Nat.le_of_not_gt fits)
    rw [h] at hr
    exact fail_not_ok hr
  · exact sub_duration_overflow t d

theorem add_time_all_inputs (n duration : U64) :
    ∃ r, LedgerPanicPayload.add_time n duration = .ok r ∧
      r.timestamp_nanos.val = min (UScalar.max .U64) (n.val + duration.val) := by
  obtain ⟨d, hd, hv⟩ := from_nanos_value duration
  have fits : durationValue d ≤ UScalar.max .U64 := by
    rw [hv]
    have h := duration.hBounds
    simp only [UScalar.max, UScalarTy.numBits] at h ⊢
    omega
  obtain ⟨r, hr, hrv⟩ := add_success ⟨n⟩ d fits
  refine ⟨r, ?_, ?_⟩
  · simpa [LedgerPanicPayload.add_time,
      timestamp.TimeStamp.from_nanos_since_unix_epoch, hd] using hr
  · simpa [hv] using hrv

theorem sub_time_all_inputs (n duration : U64) :
    ∃ r, LedgerPanicPayload.sub_time n duration = .ok r ∧
      r.timestamp_nanos.val = n.val - duration.val := by
  obtain ⟨d, hd, hv⟩ := from_nanos_value duration
  have fits : durationValue d ≤ UScalar.max .U64 := by
    rw [hv]
    have h := duration.hBounds
    simp only [UScalar.max, UScalarTy.numBits] at h ⊢
    omega
  obtain ⟨r, hr, hrv⟩ := sub_success ⟨n⟩ d fits
  refine ⟨r, ?_, ?_⟩
  · simpa [LedgerPanicPayload.sub_time,
      timestamp.TimeStamp.from_nanos_since_unix_epoch, hd] using hr
  · simpa [hv] using hrv

theorem durationValue_fields (d : core.time.Duration) :
    durationValue d / 1000000000 = d.secs.val ∧
    durationValue d % 1000000000 = d.nanos.val.val := by
  have h := d.nanos.property.2
  unfold durationValue
  omega

theorem from_nanos_fields (n : U64) :
    ∃ d, core.time.Duration.from_nanos n = .ok d ∧
      d.secs.val = n.val / 1000000000 ∧ d.nanos.val.val = n.val % 1000000000 := by
  obtain ⟨d, hd, hv⟩ := from_nanos_value n
  have hf := durationValue_fields d
  exact ⟨d, hd, by omega, by omega⟩

theorem from_nanos_as_nanos_all_inputs (n : U64) :
    (do let d ← core.time.Duration.from_nanos n; core.time.Duration.as_nanos d) =
      .ok (UScalar.cast .U128 n) := by
  obtain ⟨d, hd, hv⟩ := from_nanos_value n
  obtain ⟨u, hu, huv⟩ := as_nanos_value d
  have he : u = UScalar.cast .U128 n := by
    apply UScalar.eq_of_val_eq
    simpa [hv] using huv
  simp [hd, hu, he]

theorem new_unchecked_precondition (n : U64) :
    ∃ rem : U64, (n % 1000000000#u64 : Result U64) = Result.ok rem ∧
      (UScalar.cast .U32 rem).val < 1000000000 := by
  obtain ⟨rem, hr, hv⟩ := WP.spec_imp_exists
    (UScalar.rem_spec n (y := 1000000000#u64) (by decide))
  have bound : rem.val < 1000000000 := by
    rw [hv]
    exact Nat.mod_lt _ (by decide)
  have hc : (UScalar.cast .U32 rem).val = rem.val := by
    apply UScalar.cast_val_mod_pow_of_inBounds_eq
    simp only [UScalarTy.numBits]
    omega
  exact ⟨rem, hr, by omega⟩
end LedgerPanicPayloadCorrespondence
