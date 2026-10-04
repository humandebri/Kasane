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
  List.iter (fun make ->
    let actual = TypesAnalysis.analyze_ty None TypeDeclId.Map.empty (make ty) in
    let expected = TypesAnalysis.analyze_ty None TypeDeclId.Map.empty (make base) in
    if actual <> expected then failwith "range changed borrow classification";
    let outlive x = TypesAnalysis.compute_outlive_proj_ty None TypeDeclId.Map.empty
      RegionId.Set.empty x in
    if not (RegionId.Set.equal (outlive (make ty)) (outlive (make base))) then
      failwith "range changed outlive classification") wrap;
  if before <> show_ty ty then failwith "range type or bounds were rewritten"
let reject base pat =
  if TypesAnalysis.supported_u32_range_pattern base pat then failwith "unsupported range accepted";
  let failed = try ignore (TypesAnalysis.analyze_ty None TypeDeclId.Map.empty
                          (TPattern (base, pat))); false with
    | Errors.CFailure error when String.starts_with ~prefix:"unsupported type:" error.msg -> true
    | Failure msg when String.starts_with ~prefix:"unsupported type:" msg -> true in
  if not failed then failwith "strict rejection was relaxed"

let () =
  let crate = match LlbcOfJson.crate_of_json_file Sys.argv.(1) with
    | Ok c -> c | Error e -> failwith e in
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
                  incr casts
              | _ -> failwith "actual unsafe cast body changed")
         | _ -> failwith "actual unsafe cast body must stay transparent")
    | _ -> ()) crate.fun_decls;
  if !casts <> 2 then failwith "expected both actual scalar/Nanoseconds cast bodies";
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
  print_endline "actual Nanoseconds plus four U32 boundary ranges: 20 borrow/outlive comparisons, 12 strict negatives; exact range types and both actual typed transmute bodies retained"
