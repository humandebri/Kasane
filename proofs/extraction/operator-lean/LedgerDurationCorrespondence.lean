import LedgerDurationExtract.Funs
open Aeneas Aeneas.Std
namespace LedgerDurationCorrespondence
open LedgerDuration

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

theorem from_nanos_value (n : U64) :
    ∃ d, core.time.Duration.from_nanos n = .ok d ∧ durationValue d = n.val := by
  refine ⟨_, rfl, ?_⟩
  simp only [durationValue, U64.ofNatCore_val_eq, U32.ofNatCore_val_eq]
  omega

theorem as_nanos_value (d : core.time.Duration) :
    ∃ u, core.time.Duration.as_nanos d = .ok u ∧ u.val = durationValue d := by
  exact ⟨_, rfl, U128.ofNatCore_val_eq _⟩

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
      hu, hc, LedgerDuration.core.result.Result.unwrap, lift]
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
      hu, hc, LedgerDuration.core.result.Result.unwrap, lift]
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
    hu, hc, LedgerDuration.core.result.Result.unwrap]

theorem sub_duration_overflow (t : timestamp.TimeStamp) (d : core.time.Duration)
    (over : UScalar.max .U64 < durationValue d) :
    timestamp.TimeStamp.Insts.CoreOpsArithSubDurationTimeStamp.sub t d = .fail .panic := by
  obtain ⟨u, hu, hv⟩ := as_nanos_value d
  have hc := try_from_overflow u (by simpa [hv] using over)
  simp [timestamp.TimeStamp.Insts.CoreOpsArithSubDurationTimeStamp.sub,
    hu, hc, LedgerDuration.core.result.Result.unwrap]

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
    ∃ r, LedgerDuration.add_time n duration = .ok r ∧
      r.timestamp_nanos.val = min (UScalar.max .U64) (n.val + duration.val) := by
  obtain ⟨d, hd, hv⟩ := from_nanos_value duration
  have fits : durationValue d ≤ UScalar.max .U64 := by
    rw [hv]
    have h := duration.hBounds
    simp only [UScalar.max, UScalarTy.numBits] at h ⊢
    omega
  obtain ⟨r, hr, hrv⟩ := add_success ⟨n⟩ d fits
  refine ⟨r, ?_, ?_⟩
  · simpa [LedgerDuration.add_time,
      timestamp.TimeStamp.from_nanos_since_unix_epoch, hd] using hr
  · simpa [hv] using hrv

theorem sub_time_all_inputs (n duration : U64) :
    ∃ r, LedgerDuration.sub_time n duration = .ok r ∧
      r.timestamp_nanos.val = n.val - duration.val := by
  obtain ⟨d, hd, hv⟩ := from_nanos_value duration
  have fits : durationValue d ≤ UScalar.max .U64 := by
    rw [hv]
    have h := duration.hBounds
    simp only [UScalar.max, UScalarTy.numBits] at h ⊢
    omega
  obtain ⟨r, hr, hrv⟩ := sub_success ⟨n⟩ d fits
  refine ⟨r, ?_, ?_⟩
  · simpa [LedgerDuration.sub_time,
      timestamp.TimeStamp.from_nanos_since_unix_epoch, hd] using hr
  · simpa [hv] using hrv
end LedgerDurationCorrespondence
