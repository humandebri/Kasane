import TypeInfoGenerated
open Aeneas Aeneas.Std
open CharonTypeInfo.ast.type_level.type_info
namespace TypeInfoCorrespondence

abbrev Info := TypeInfo

def hasFlag (bits mask : U8) : Bool := decide (UScalar.and bits mask = mask)

theorem contains_all_inputs (bits mask : U8) :
    CharonTypeInfo.ast.type_level.type_info._.TypeFlags.contains bits mask =
      .ok (hasFlag bits mask) := by
  simp [CharonTypeInfo.ast.type_level.type_info._.TypeFlags.contains,
    CharonTypeInfo.ast.type_level.type_info._.InternalBitFlags.contains,
    CharonTypeInfo.ast.type_level.type_info._.InternalBitFlags.bits, hasFlag, lift, HAnd.hAnd]


theorem free_var_mask : TypeFlags.MENTIONS_FREE_VAR = .ok (16#u8) := by
  unfold TypeFlags.MENTIONS_FREE_VAR
  simp only [HShiftLeft.hShiftLeft, UScalar.shiftLeft_IScalar, UScalar.shiftLeft,
    CharonTypeInfo.ast.type_level.type_info._.TypeFlags.from_bits_retain,
    CharonTypeInfo.ast.type_level.type_info._.InternalBitFlags.from_bits_retain,
    IScalar.toNat]
  norm_num [IScalar.val, IScalarTy.I32_numBits_eq, UScalarTy.U8_numBits_eq]
  congr 1

theorem self_clause_mask : TypeFlags.MENTIONS_SELF_CLAUSE = .ok (2#u8) := by
  unfold TypeFlags.MENTIONS_SELF_CLAUSE
  simp only [HShiftLeft.hShiftLeft, UScalar.shiftLeft_IScalar, UScalar.shiftLeft,
    CharonTypeInfo.ast.type_level.type_info._.TypeFlags.from_bits_retain,
    CharonTypeInfo.ast.type_level.type_info._.InternalBitFlags.from_bits_retain,
    IScalar.toNat]
  norm_num [IScalar.val, IScalarTy.I32_numBits_eq, UScalarTy.U8_numBits_eq]
  congr 1

theorem metadata_mask : TypeFlags.USES_SIZE_METADATA = .ok (4#u8) := by
  unfold TypeFlags.USES_SIZE_METADATA
  simp only [HShiftLeft.hShiftLeft, UScalar.shiftLeft_IScalar, UScalar.shiftLeft,
    CharonTypeInfo.ast.type_level.type_info._.TypeFlags.from_bits_retain,
    CharonTypeInfo.ast.type_level.type_info._.InternalBitFlags.from_bits_retain,
    IScalar.toNat]
  norm_num [IScalar.val, IScalarTy.I32_numBits_eq, UScalarTy.U8_numBits_eq]
  congr 1

theorem mentions_var_all_inputs (info : Info) :
    TypeInfo.mentions_var info =
      .ok (info.max_de_bruijn_id.isSome || hasFlag info.flags (16#u8)) := by
  unfold TypeInfo.mentions_var
  rw [free_var_mask]
  cases info.max_de_bruijn_id <;>
    simp [core.option.Option.is_some, contains_all_inputs]

theorem self_clause_all_inputs (info : Info) :
    TypeInfo.mentions_self_clause info = .ok (hasFlag info.flags (2#u8)) := by
  unfold TypeInfo.mentions_self_clause
  rw [self_clause_mask]
  simp [contains_all_inputs]

theorem metadata_all_inputs (info : Info) :
    TypeInfo.uses_size_metadata info = .ok (hasFlag info.flags (4#u8)) := by
  unfold TypeInfo.uses_size_metadata
  rw [metadata_mask]
  simp [contains_all_inputs]

def closedSpec (info : Info) : Bool :=
  !info.max_de_bruijn_id.isSome && !hasFlag info.flags (16#u8) &&
    !hasFlag info.flags (2#u8) && !hasFlag info.flags (4#u8)

theorem closed_all_inputs (info : Info) :
    TypeInfo.is_closed info = .ok (closedSpec info) := by
  unfold TypeInfo.is_closed
  rw [mentions_var_all_inputs]
  simp only [bind_ok]
  rw [self_clause_all_inputs, metadata_all_inputs]
  cases h0 : info.max_de_bruijn_id.isSome <;>
    cases h1 : hasFlag info.flags (16#u8) <;>
    cases h2 : hasFlag info.flags (2#u8) <;>
    cases h3 : hasFlag info.flags (4#u8) <;>
    simp [closedSpec, h0, h1, h2, h3]

end TypeInfoCorrespondence
