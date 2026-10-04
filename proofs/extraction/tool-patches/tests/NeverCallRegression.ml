open Aeneas
open Types
open LlbcAst
open Expressions

let scalar = TScalar (TInteger (Unsigned U32))
let scalar64 = TScalar (TInteger (Unsigned U64))
let c s : constant_expr = { kind = CInteger (UnsignedInteger (U32, Z.of_string s)); ty = scalar }
let range lo hi = Range (c lo, c hi)
let wrap = [ (fun x -> x); (fun x -> TRef (RStatic, x, RShared));
             (fun x -> TRef (RStatic, x, RMut));
             (fun x -> TRef (RStatic, TRef (RStatic, x, RMut), RShared)) ]
let check_supported base pat =
  if not (TypesAnalysis.supported_u32_range_pattern base pat) then
    failwith "expected supported well-typed constant U32 range";
  let ty = TPattern (base, pat) in
  let before = show_ty ty in
  let translated = SymbolicToPureTypes.translate_sty None ty in
  (match pat, translated with
   | Range (lo, hi), Pure.TAdt (Pure.TBuiltin Pure.TU32Range,
       {types = []; trait_refs = []; const_generics = [lo_value; hi_value]}) ->
       if lo_value <> SymbolicToPureTypes.translate_constant_expr_kind None lo.kind
          || hi_value <> SymbolicToPureTypes.translate_constant_expr_kind None hi.kind then
         failwith "Pure range endpoints were rewritten"
   | _ -> failwith "range constraint not represented in Pure type");
  List.iter (fun make ->
    let actual = TypesAnalysis.analyze_ty None TypeDeclId.Map.empty (make ty) in
    let expected = TypesAnalysis.analyze_ty None TypeDeclId.Map.empty (make base) in
    if actual <> expected then failwith "range changed borrow classification";
    let outlive x = TypesAnalysis.compute_outlive_proj_ty None TypeDeclId.Map.empty
      RegionId.Set.empty x in
    if not (RegionId.Set.equal (outlive (make ty)) (outlive (make base))) then
      failwith "range changed outlive classification") wrap;
  if before <> show_ty ty then failwith "range type or bounds were rewritten"
let translation_rejected ty =
  try ignore (SymbolicToPureTypes.translate_sty None ty); false with
  | Errors.CFailure _ | Failure _ -> true

let reject base pat =
  if TypesAnalysis.supported_u32_range_pattern base pat then failwith "unsupported range accepted";
  let failed = try ignore (TypesAnalysis.analyze_ty None TypeDeclId.Map.empty
                          (TPattern (base, pat))); false with
    | Errors.CFailure error when String.starts_with ~prefix:"unsupported type:" error.msg -> true
    | Failure msg when String.starts_with ~prefix:"unsupported type:" msg -> true in
  if not failed then failwith "strict rejection was relaxed";
  if not (translation_rejected (TPattern (base, pat))) then
    failwith "Pure translator accepted unsupported range"

let () =
  Config.opt_backend := Some Config.Lean;
  let crate = match LlbcOfJson.crate_of_json_file Sys.argv.(1) with
    | Ok c -> c | Error e -> failwith e in
  let error = List.find (fun (d : type_decl) ->
      List.filter_map (function PeIdent (n, _) -> Some n | _ -> None) d.item_meta.name
       = ["core"; "num"; "error"; "TryFromIntError"])
      (List.map snd (TypeDeclId.Map.bindings crate.type_decls)) in
  let field = match error.kind with Struct [f] -> f | _ -> failwith "error field absent" in
  let payload_ref = match field.field_ty with TAdt r -> r | _ -> failwith "error payload absent" in
  let payload = TypeDeclId.Map.find payload_ref.id crate.type_decls in
  let guard defs d = TypesAnalysis.supported_try_from_int_error defs d in
  if not (guard crate.type_decls error) then failwith "actual error type rejected";
  let bad_error = [
    {error with kind = Opaque}; {error with kind = Struct []};
    {error with kind = Struct [field; field]};
    {error with kind = Struct [{field with field_ty = scalar}]};
    {error with ptr_metadata = Length};
    {error with kind = Struct [{field with field_ty = TAdt {payload_ref with
       generics = {payload_ref.generics with types = [scalar]}}}]}] in
  List.iter (fun d -> if guard crate.type_decls d then failwith "invalid error newtype accepted") bad_error;
  let variants = match payload.kind with Enum vs -> vs | _ -> failwith "error enum absent" in
  let first = List.hd variants and rest = List.tl variants in
  let bad_payload = [
    {payload with kind = Opaque}; {payload with kind = Enum []};
    {payload with kind = Enum (List.rev variants)};
    {payload with kind = Enum ({first with variant_name = "Other"} :: rest)};
    {payload with kind = Enum ({first with fields = [field]} :: rest)};
    {payload with kind = Enum ({first with discriminant = SignedInteger (Isize, Z.one)} :: rest)};
    {payload with ptr_metadata = Length}] in
  List.iter (fun d -> if guard (TypeDeclId.Map.add payload_ref.id d crate.type_decls) error then
      failwith "invalid IntErrorKind accepted") bad_payload;
  let unwanted = NameMatcher.parse_pattern "core::num::error::TryFromIntError" in
  if List.exists (fun (i : Pure.builtin_type_info) -> i.rust_name = unwanted)
      (ExtractBuiltin.builtin_types ()) then failwith "Unit builtin retained";
  let actual_builtins = ExtractBuiltin.builtin_types () in
  List.iter (fun (expected : Pure.builtin_type_info) ->
    if expected.rust_name <> unwanted && not (List.exists ((=) expected) actual_builtins) then
      failwith "unrelated Lean builtin mapping changed") ExtractBuiltinLean.lean_builtin_types;
  print_endline "actual error newtype/payload retained; 13 malformed definitions rejected; Unit builtin removed; all other Lean builtin mappings unchanged";
  let found = ref 0 in
  let nano_id = ref None in
  TypeDeclId.Map.iter (fun _ (d : type_decl) ->
    if List.filter_map (function PeIdent (name, _) -> Some name | _ -> None)
         d.item_meta.name = ["core"; "num"; "niche_types"; "Nanoseconds"] then (
      match d.kind with
      | Struct [field] -> (match field.field_ty with
          | TPattern (base, (Range (lo, hi) as pat)) ->
              if base <> scalar || lo <> c "0" || hi <> c "999999999" then
                failwith "actual Nanoseconds range changed";
              check_supported base pat; nano_id := Some d.def_id; incr found
          | _ -> failwith "actual Nanoseconds range missing")
      | _ -> failwith "actual Nanoseconds must remain transparent single-field struct")) crate.type_decls;
  if !found <> 1 then failwith "expected exactly one actual Nanoseconds range";
  let nano_id = Option.get !nano_id in
  let is_nano = function
    | TAdt tref -> tref.id = nano_id && tref.builtin = None
                   && tref.generics = Charon.TypesUtils.empty_generic_args
    | _ -> false in
  let local n (p : place) = match p.kind with
    | PlaceLocal id -> LocalId.to_int id = n | _ -> false in
  let actual_nano_ty = ref None in
  let casts = ref 0 in
  FunDeclId.Map.iter (fun _ (f : fun_decl) ->
    let name = List.filter_map (function PeIdent (n, _) -> Some n | _ -> None)
      f.item_meta.name in
    match name with
    | ["core"; "num"; "niche_types"; method_] when
         method_ = "new_unchecked" || method_ = "as_inner" ->
        let is_new = method_ = "new_unchecked" in
        if f.signature.is_unsafe <> is_new then failwith "unsafe signature changed";
        (match f.body with
         | StructuredBody b ->
             let active = List.filter (fun (st : statement) ->
               match st.kind with StorageLive _ | StorageDead _ -> false | _ -> true)
               b.body.statements in
             (match active with
              | [{kind = Assign (tmp, Use (Copy input, YesRetag)); _};
                 {kind = Assign (out, UnaryOp (Cast (CastTransmute (src, tgt)), Move op)); _};
                 {kind = Return; _}] ->
                  if not (local 2 tmp && local 1 input && local 0 out && local 2 op)
                     || tmp.ty <> src || input.ty <> src || op.ty <> src || out.ty <> tgt
                     || f.signature.inputs <> [src] || f.signature.output <> tgt then
                    failwith "actual cast places or signature types changed";
                  if not (if is_new then src = scalar && is_nano tgt
                                      else is_nano src && tgt = scalar) then
                    failwith "actual transmute direction changed";
                  if not is_new then actual_nano_ty := Some src;
                  incr casts
              | _ -> failwith "actual unsafe cast body changed")
         | _ -> failwith "actual unsafe cast body must stay transparent")
    | _ -> ()) crate.fun_decls;
  if !casts <> 2 then failwith "expected both actual scalar/Nanoseconds cast bodies";
  let src = Option.get !actual_nano_ty in
  let d = TypeDeclId.Map.find nano_id crate.type_decls in
  let read_ok decl = TypesAnalysis.supported_nanoseconds_read
    (TypeDeclId.Map.add nano_id decl crate.type_decls) src scalar in
  if not (read_ok d) then failwith "actual native Nanoseconds read rejected";
  let construct_ok decl = TypesAnalysis.supported_nanoseconds_construct
    (TypeDeclId.Map.add nano_id decl crate.type_decls) scalar src in
  if not (construct_ok d) then failwith "actual constructor layout rejected";
  if TypesAnalysis.supported_nanoseconds_construct crate.type_decls src scalar then
    failwith "constructor guard accepted reverse direction";
  if TypesAnalysis.supported_nanoseconds_construct crate.type_decls scalar64 src then
    failwith "constructor guard accepted wrong scalar";
  if TypesAnalysis.supported_nanoseconds_read crate.type_decls scalar src then
    failwith "reverse unsafe constructor accepted";
  if TypesAnalysis.supported_nanoseconds_read crate.type_decls src scalar64 then
    failwith "wrong target scalar accepted";
  let target, l = List.hd d.layout in
  let layout_bad l = { d with layout = [target, l] } in
  let constant_size n : size = {l.size with chosen = Some
      (SizeExprConstant {kind = CInteger (UnsignedInteger (Usize, Z.of_int n));
                         ty = TScalar (TInteger (Unsigned Usize))})} in
  let v = Option.get (List.hd l.variant_layouts) in
  let variants v = layout_bad {l with variant_layouts = [Some v]} in
  let field = match d.kind with Struct [f] -> f | _ -> failwith "field missing" in
  let field_ty ty = {d with kind = Struct [{field with field_ty = ty}]} in
  let wrong_name = List.map (function
      | PeIdent ("Nanoseconds", dis) -> PeIdent ("Other", dis) | x -> x) d.item_meta.name in
  let bad = [
    field_ty scalar; field_ty (TPattern (scalar, range "1" "999999999"));
    field_ty (TPattern (scalar, range "0" "999999998"));
    {d with item_meta = {d.item_meta with name = wrong_name}};
    layout_bad {l with size = {l.size with chosen = None}};
    layout_bad {l with align = {l.align with chosen = None}};
    {d with layout = [target, l; "wasm32-unknown-unknown", l]};
    {d with layout = []}; {d with layout = ["wasm32-unknown-unknown", l]};
    layout_bad {l with size = constant_size 8};
    layout_bad {l with align = constant_size 8};
    layout_bad {l with repr = {l.repr with transparent = false}};
    layout_bad {l with repr = {l.repr with repr_algo = C}};
    layout_bad {l with inhabited = InhabitedPredicateFalse};
    layout_bad {l with discriminator = None};
    layout_bad {l with variant_layouts = []};
    variants {v with inhabited = InhabitedPredicateFalse};
    variants {v with field_offsets = []};
    variants {v with field_offsets = [{chosen = Some 4; guarantee = None}]};
    variants {v with field_offsets = [{chosen = None; guarantee = None}]};
    variants {v with tagger = [0, UnsignedInteger (U32, Z.zero)]};
    {d with kind = Opaque}; {d with kind = Struct []};
    {d with ptr_metadata = Length}] in
  List.iter (fun x -> if read_ok x || construct_ok x then failwith "invalid Nanoseconds layout accepted") bad;
  print_endline "actual native layout read and construct guards passed; 24 metadata negatives, opposite directions and wrong scalars rejected";

  List.iter (fun (lo, hi) -> check_supported scalar (range lo hi))
    ["0", "0"; "1", "1"; "0", "4294967295"; "4294967295", "4294967295"];
  let lo = c "0" and hi = c "999999999" in
  List.iter (fun (base, pat) -> reject base pat)
    [scalar, range "-1" "1"; scalar, range "0" "4294967296";
     scalar, range "2" "1"; scalar, Range ({lo with ty = scalar64}, hi);
     scalar, Range (lo, {hi with ty = scalar64});
     scalar, Range ({lo with kind = CInteger (UnsignedInteger (U64, Z.zero))}, hi);
     scalar, Range (lo, {hi with kind = CInteger (UnsignedInteger (U64, Z.one))});
     scalar, Range ({lo with kind = CBool false}, hi);
     scalar64, Range (lo, hi); TScalar TBool, Range (lo, hi);
     scalar, OrPattern [Range (lo, hi)]; scalar, NotNull];
  List.iter (fun backend ->
    Config.opt_backend := Some backend;
    if not (translation_rejected (TPattern (scalar, range "0" "999999999"))) then
      failwith "non-Lean backend accepted isolated range support")
    [Config.FStar; Config.Coq; Config.HOL4];
  Config.opt_backend := Some Config.Lean;
  print_endline "actual Nanoseconds plus four U32 boundary ranges: 20 borrow/outlive comparisons, 12 strict negatives; exact range types, Pure endpoints and both actual typed transmute bodies retained; 12 Pure negatives and three non-Lean backends rejected"


let () =
  Config.opt_backend := Some Config.Lean;
  let crate = match LlbcOfJson.crate_of_json_file Sys.argv.(1) with
    | Ok c -> c | Error e -> failwith e in
  let _, decl = FunDeclId.Map.choose crate.fun_decls in
  let span = decl.item_meta.span in
  let removed = NameMatcher.parse_pattern
    "core::result::{core::result::Result<@T, @E>}::unwrap" in
  let actual = ExtractBuiltin.mk_builtin_funs () in
  if List.exists (fun (p, _) -> p = removed) actual then
    failwith "unwrap still mapped to builtin instead of its source body";
  List.iter (fun (p, info) ->
    if p <> removed && not (List.mem (p, info) actual) then
      failwith "another Lean builtin function registration changed")
    ExtractBuiltinLean.lean_builtin_funs;
  let scrut : Pure.texpr = {e = Pure.EError (None, "regression"); ty = Pure.TNever} in
  let output = PureUtils.mk_result_ty PureUtils.mk_unit_ty in
  let empty = PureUtils.mk_switch_with_empty_ty __FILE__ __LINE__ span output scrut (Pure.Match []) in
  if empty.ty <> output then failwith "Never elimination changed output type";
  let rejected = try
    ignore (PureUtils.mk_switch_with_empty_ty __FILE__ __LINE__ span output
      {scrut with ty = PureUtils.mk_unit_ty} (Pure.Match [])); false
    with Errors.CFailure _ | Failure _ -> true in
  if not rejected then failwith "inhabited empty match accepted";
  print_endline "Never output retained; inhabited empty match rejected; only unwrap builtin removed; all other Lean builtin functions unchanged"


let () =
  let crate = match LlbcOfJson.crate_of_json_file Sys.argv.(1) with
    | Ok c -> c | Error e -> failwith e in
  let _, decl = FunDeclId.Map.choose crate.fun_decls in
  List.iter (fun text ->
    List.iter (fun inside ->
      let actual = Format.asprintf "%t"
        (fun fmt -> ExtractTypes.extract_str decl.item_meta.span fmt ~inside text) in
      let bytes = List.init (String.length text)
        (fun i -> string_of_int (Char.code text.[i]) ^ "#u8") in
      let expected = "Slice.from [" ^ String.concat ", " bytes ^ "] (by scalar_tac)" in
      let expected = if inside then "(" ^ expected ^ ")" else expected in
      if actual <> expected then failwith "source string bytes rewritten") [false; true])
    [""; "called `Result::unwrap()` on an `Err` value"; "\000"; "\n";
     "\"\\"; "証明"; "🙂"; String.init 256 Char.chr];
  print_endline "16 string emissions preserve every source byte, including UTF-8, NUL, quoting and all 256 byte values; explicit kernel length proof"
