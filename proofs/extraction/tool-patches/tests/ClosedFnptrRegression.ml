open Aeneas
open Types
let pat = NameMatcher.parse_pattern
let removed_types = List.map pat ["core::num::error::TryFromIntError";
  "core::fmt::Arguments"; "core::fmt::rt::Argument"]
let removed_funs = List.map pat ["core::result::{core::result::Result<@T, @E>}::unwrap";
  "core::fmt::rt::{core::fmt::rt::Argument<'0>}::new_debug";
  "core::fmt::rt::{core::fmt::rt::Argument<'0>}::new_display";
  "core::fmt::{core::fmt::Arguments<'a>}::new"]
let () =
  Config.opt_backend := Some Config.Lean;
  let c = match LlbcOfJson.crate_of_json_file Sys.argv.(1) with
    | Ok c -> c | Error e -> failwith e in
  if List.length (List.filter (fun (d : Pure.builtin_type_info) ->
      List.mem d.rust_name removed_types) ExtractBuiltinLean.lean_builtin_types) <> 3 then
    failwith "expected source type registrations absent";
  if List.length (List.filter (fun (p,_) -> List.mem p removed_funs)
      ExtractBuiltinLean.lean_builtin_funs) <> 4 then
    failwith "expected source function registrations absent";
  let ts = ExtractBuiltin.builtin_types () in
  List.iter (fun (d : Pure.builtin_type_info) ->
    if List.mem d.rust_name removed_types then (
      if List.exists (fun (x : Pure.builtin_type_info) -> x.rust_name=d.rust_name) ts then
        failwith "source type still replaced by builtin")
    else if not (List.mem d ts) then failwith "unrelated type mapping changed")
    ExtractBuiltinLean.lean_builtin_types;
  let fs = ExtractBuiltin.mk_builtin_funs () in
  List.iter (fun (p,i) ->
    if List.mem p removed_funs then (
      if List.exists (fun (q,_) -> p=q) fs then
        failwith "source function still replaced by builtin")
    else if not (List.mem (p,i) fs) then failwith "unrelated function mapping changed")
    ExtractBuiltinLean.lean_builtin_funs;
  let pointers = ref [] in
  let visitor = object (self)
    inherit [_] LlbcAstUtils.iter_crate as super
    method! visit_ty () ty =
      (match ty with TFnPtr _ -> pointers := ty :: !pointers | _ -> ());
      self#visit_ty_default () ty
    method private visit_ty_default ctx ty = super#visit_ty ctx ty
  end in
  visitor#visit_crate () c;
  if List.length !pointers<>29 then failwith "unexpected source function pointer inventory";
  let accepted = ref 0 and rejected = ref 0 in
  List.iter (fun ty ->
    let before = show_ty ty in
    let sg = match ty with TFnPtr s -> s | _ -> assert false in
    if SymbolicToPureTypes.supported_closed_rust_fnptr sg then (
      let pure = SymbolicToPureTypes.translate_sty None ty in
      (match pure with
       | Pure.TAdt (Pure.TBuiltin (Pure.TRustFnPtr actual), args) ->
          if actual<>sg || args.types<>[] || args.trait_refs<>[] || args.const_generics<>[] then
            failwith "source signature or marker arguments changed"
       | _ -> failwith "source pointer lowered to an arrow or erased type");
      incr accepted)
    else (
      let failed = try ignore (SymbolicToPureTypes.translate_sty None ty); false
        with Errors.CFailure _ | Failure _ -> true in
      if not failed then failwith "open signature accepted";
      incr rejected);
    if before<>show_ty ty then failwith "source metadata altered") !pointers;
  if !accepted=0 || !rejected=0 then failwith "missing actual closed/open source signatures";
  let good = List.find (fun s -> SymbolicToPureTypes.supported_closed_rust_fnptr s)
    (List.map (function TFnPtr s -> s | _ -> assert false) !pointers) in
  let sg = good.binder_value in
  let first = List.hd good.binder_regions in
  let input t = {good with binder_value = {sg with inputs = [t]}} in
  let ref_input r = input (TRef (r, TScalar (TInteger (Unsigned U32)), RShared)) in
  let bad = [
    {good with binder_value = {sg with abi = AbiC}};
    {good with binder_value = {sg with is_variadic = true}};
    {good with binder_regions = []};
    {good with binder_regions = [first; first]};
    ref_input RErased; ref_input (RVar (Free (RegionId.of_int 0)));
    ref_input (RVar (Bound (1, RegionId.of_int 0)));
    ref_input (RVar (Bound (0, RegionId.of_int (List.length good.binder_regions))));
    ref_input (RVar (Bound (0, RegionId.of_int (-1))));
    input (TVar (Free (TypeVarId.of_int 0)));
    {good with binder_value = {sg with output = TVar (Free (TypeVarId.of_int 0))}};
    input (TFnPtr good)] in
  List.iter (fun b ->
    if SymbolicToPureTypes.supported_closed_rust_fnptr b then failwith "open/bad signature accepted";
    let failed = try ignore (SymbolicToPureTypes.translate_sty None (TFnPtr b)); false
      with Errors.CFailure _ | Failure _ -> true in
    if not failed then failwith "bad signature translated") bad;
  let translate s = SymbolicToPureTypes.translate_sty None (TFnPtr s) in
  let original = translate good in
  let toggled = {good with binder_value = {sg with is_unsafe = not sg.is_unsafe}} in
  if translate toggled=original then failwith "safe/unsafe signatures collapsed";
  let subst : PureUtils.subst = {
    ty_subst = (fun _ -> failwith "closed pointer leaked a type variable");
    cg_subst = (fun _ -> failwith "closed pointer leaked a const variable");
    tr_subst = (fun _ -> failwith "closed pointer leaked a trait variable");
    tr_self = Pure.Self} in
  if PureUtils.ty_substitute subst original<>original then failwith "closed pointer changed on substitution";
  List.iter (fun b ->
    Config.opt_backend := Some b;
    let failed = try ignore (translate good); false with Errors.CFailure _ | Failure _ -> true in
    if not failed then failwith "unsupported backend accepted pointer marker")
    [Config.Coq; Config.FStar; Config.HOL4];
  Config.opt_backend := Some Config.Lean;
  Printf.printf "%d malformed/open signatures and three non-Lean backends rejected; unsafe flag distinct; closed marker substitution unchanged\n" (List.length bad);
  Printf.printf "closed signatures retained exactly: %d; open signatures rejected: %d\n" !accepted !rejected;
  Printf.printf "actual function pointer signatures retained (%d occurrences); source metadata checked\n" (List.length !pointers);
  print_endline "exact three type/four function mappings removed; every other Lean mapping unchanged"
