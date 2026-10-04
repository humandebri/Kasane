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

let () =
  let c = match LlbcOfJson.crate_of_json_file Sys.argv.(1) with
    | Ok c -> c | Error e -> failwith e in
  let defs = ref [] and ptrs = ref [] in
  let visitor = object
    inherit [_] LlbcAstUtils.iter_crate as super
    method! visit_ty () ty =
      (match ty with TFnDef _ -> defs := ty :: !defs
       | TFnPtr _ -> ptrs := ty :: !ptrs | _ -> ());
      super#visit_ty () ty
  end in
  visitor#visit_crate () c;
  if !defs=[] || !ptrs=[] then failwith "actual function type metadata missing";
  List.iter (fun ty ->
    let before = show_ty ty in
    if not (TypesUtils.ty_is_rty ty) then failwith ("valid function binder rejected: "^before);
    if before<>show_ty ty then failwith "function signature rewritten";
    if Substitute.erase_regions ty<>ty then failwith "bound function regions erased") (!defs @ !ptrs);
  Printf.printf "actual function regions are scoped without rewriting: %d FnDef, %d FnPtr\n"
    (List.length !defs) (List.length !ptrs)

let () =
  let c = match LlbcOfJson.crate_of_json_file Sys.argv.(1) with
    | Ok c -> c | Error e -> failwith e in
  let sample = ref None in
  let visitor = object
    inherit [_] LlbcAstUtils.iter_crate as super
    method! visit_ty () ty =
      (match ty with TFnPtr s when !sample=None -> sample := Some s | _ -> ());
      super#visit_ty () ty
  end in
  visitor#visit_crate () c;
  let base = Option.get !sample in
  let scalar = TScalar (TInteger (Unsigned U32)) in
  let make depth id =
    TFnPtr {base with binder_value = {base.binder_value with
      inputs = [TRef (RVar (Bound (depth,RegionId.of_int id)),scalar,RShared)];
      output = scalar}} in
  let count = List.length base.binder_regions in
  let checked = ref 0 in
  List.iter (fun depth -> List.iter (fun id ->
    let ty = make depth id in
    let expected = depth=0 && id>=0 && id<count in
    if TypesUtils.ty_is_rty ty<>expected then failwith "scope predicate mismatch";
    if expected then (
      if TypesUtils.erase_regions_preserve_fn_binders ty<>ty then failwith "local region lost in body erase";
      if Substitute.erase_regions ty<>ty then failwith "local region lost in library erase")
    else (
      let rejected = try ignore (TypesUtils.erase_regions_preserve_fn_binders ty);false
        with Errors.CFailure _ | Failure _ -> true in
      if not rejected then failwith "out-of-scope body region accepted");
    incr checked) [-1;0;1;2;3;4]) [-2;-1;0;1;2;3];
  let empty = {base with binder_regions = []} in
  let nested depth = TFnPtr {base with binder_value = {base.binder_value with
      inputs = [TFnPtr {empty with binder_value = {empty.binder_value with
        inputs = [TRef (RVar (Bound (depth,RegionId.of_int 0)),scalar,RShared)];
        output = scalar}}]; output = scalar}} in
  if not (TypesUtils.ty_is_rty (nested 1)) || TypesUtils.ty_is_rty (nested 0) then
    failwith "empty region binder depth ignored";
  if TypesUtils.erase_regions_preserve_fn_binders (nested 1)<>nested 1
     || Substitute.erase_regions (nested 1)<>nested 1 then failwith "nested local region erased";
  let free = TRef (RVar (Free (RegionId.of_int 99)),scalar,RShared) in
  if Substitute.erase_regions free<>TRef(RErased,scalar,RShared)
     || TypesUtils.erase_regions_preserve_fn_binders free<>TRef(RErased,scalar,RShared) then
    failwith "free region not erased";
  if TypesUtils.ty_is_rty (TRef(RErased,scalar,RShared))
     || TypesUtils.ty_is_rty (TRef(RVar(Bound(0,RegionId.of_int 0)),scalar,RShared)) then
    failwith "ordinary rty rejection relaxed";
  let free_fn = TFnPtr {base with binder_value = {base.binder_value with inputs=[free]; output=scalar}} in
  let erased_fn = TFnPtr {base with binder_value = {base.binder_value with
    inputs=[TRef(RErased,scalar,RShared)]; output=scalar}} in
  if TypesUtils.erase_regions_preserve_fn_binders free_fn<>erased_fn
     || Substitute.erase_regions free_fn<>erased_fn then failwith "free function region not erased";
  if TypesUtils.ty_is_rty erased_fn then failwith "erased function input accepted as rty";
  Printf.printf "%d depth/index scope cases; empty nested binder preserved; free regions still erased; ordinary/erased rty rejection preserved\n" !checked

let () =
  let c = match LlbcOfJson.crate_of_json_file Sys.argv.(1) with
    | Ok c -> c | Error e -> failwith e in
  let casts = ref [] in
  let visitor = object
    inherit [_] LlbcAstUtils.iter_crate as super
    method! visit_CastFnPtr () src dst =
      casts := (src, dst) :: !casts;
      super#visit_CastFnPtr () src dst
  end in
  visitor#visit_crate () c;
  if List.length !casts <> 2 then failwith "unexpected source reification inventory";
  List.iter (fun (src, dst) ->
    let fn = match src with TFnDef f -> f | _ -> failwith "source is not function item" in
    let target = match dst with TFnPtr f -> f | _ -> failwith "target is not function pointer" in
    let resolved = match Substitute.lookup_fndef_sig c fn with
      | Some s -> s | None -> failwith "source trait method signature unresolved" in
    if resolved <> target then failwith ("reified signature mismatch: " ^ show_ty (TFnPtr resolved) ^ " vs " ^ show_ty dst);
    let tr, id = match fn.binder_value.kind with TraitMethod (tr,id) -> tr,id
      | _ -> failwith "expected source trait method" in
    Printf.printf "reification signature matched exactly: %s\n" (Types.show_trait_method_id id);
    let missing = { c with trait_decls = TraitDeclId.Map.empty } in
    if Substitute.lookup_fndef_sig missing fn <> None then failwith "missing trait accepted";
    let absent = {fn with binder_value = {fn.binder_value with
      kind = TraitMethod(tr,TraitMethodId.of_int 999)}} in
    if Substitute.lookup_fndef_sig c absent <> None then failwith "missing method accepted";
    let poly_tr = {tr with trait_decl_ref = {tr.trait_decl_ref with binder_regions = fn.binder_regions}} in
    let poly_fn = {fn with binder_value = {fn.binder_value with kind = TraitMethod(poly_tr,id)}} in
    if Substitute.lookup_fndef_sig c poly_fn <> None then failwith "higher-ranked trait binder discarded";
    let unsafe_target = {target with binder_value = {target.binder_value with is_unsafe = not target.binder_value.is_unsafe}} in
    let abi_target = {target with binder_value = {target.binder_value with abi = AbiC}} in
    let variadic_target = {target with binder_value = {target.binder_value with is_variadic = not target.binder_value.is_variadic}} in
    List.iter (fun bad -> if resolved = bad then failwith "signature metadata ignored")
      [unsafe_target;abi_target;variadic_target];
    if fn.binder_regions <> resolved.binder_regions then failwith "function item binder changed") !casts;
  print_endline "two source trait reification signatures match exactly; missing trait/method/poly trait rejected; unsafe/ABI/variadic distinct"

let () =
  let c = match LlbcOfJson.crate_of_json_file Sys.argv.(1) with
    | Ok c -> c | Error e -> failwith e in
  let casts = ref [] in
  let visitor = object
    inherit [_] LlbcAstUtils.iter_crate as super
    method! visit_CastFnPtr () src dst =
      casts := (src,dst) :: !casts;
      super#visit_CastFnPtr () src dst
  end in
  visitor#visit_crate () c;
  if List.length !casts <> 2 then failwith "source reification casts missing";
  let checked = ref 0 in
  List.iter (fun (src,dst) ->
    let matches = InterpExpressions.fn_item_reification_signature_matches c in
    if not (matches src dst) then failwith "actual same-signature reification rejected";
    let target = match dst with TFnPtr t -> t | _ -> assert false in
    let item = match src with TFnDef t -> t | _ -> assert false in
    let scalar = TScalar(TInteger(Unsigned U32)) in
    let bad = [
      TFnPtr {target with binder_value = {target.binder_value with abi=AbiC}};
      TFnPtr {target with binder_value = {target.binder_value with is_unsafe=not target.binder_value.is_unsafe}};
      TFnPtr {target with binder_value = {target.binder_value with is_variadic=true}};
      TFnPtr {target with binder_value = {target.binder_value with inputs=[]}};
      TFnPtr {target with binder_value = {target.binder_value with output=scalar}};
      TFnPtr {target with binder_regions=[]};
      scalar] in
    List.iter (fun bad -> if matches src bad then failwith "bad cast target accepted"; incr checked) bad;
    if matches dst dst || matches scalar dst then failwith "non-item source accepted";
    if InterpExpressions.fn_item_reification_signature_matches
      {c with trait_decls=TraitDeclId.Map.empty} src dst then failwith "unresolved trait cast accepted";
    if matches (TFnDef {item with binder_regions=[]}) dst then failwith "source binder erased";
    if matches (TFnDef {item with binder_value={item.binder_value with generics=TypesUtils.empty_generic_args}}) dst
      then failwith "source generics erased") !casts;
  Printf.printf "two actual reification guards accepted; %d target mutations rejected; non-item/missing trait/source binder/source generics controls rejected\n" !checked

let () =
  let c = match LlbcOfJson.crate_of_json_file Sys.argv.(1) with
    | Ok c -> c | Error e -> failwith e in
  let checked = ref 0 and sample = ref None in
  let visitor = object
    inherit [_] LlbcAstUtils.iter_crate as super
    method! visit_ty () ty =
      (match ty with
       | TFnDef _ | TFnPtr _ ->
           if not (RegionId.Set.is_empty (TypesUtils.ty_regions ty)) then failwith "local source region counted as free";
           if TypesUtils.ty_erase_regions ty <> ty then failwith "value type erasure lost source binder";
           if not (TypesUtils.ty_is_ety ty) then failwith "local function value rejected as ety";
           incr checked;
           (match ty with TFnPtr s -> sample := Some s | _ -> ())
       | _ -> ());
      super#visit_ty () ty
  end in
  visitor#visit_crate () c;
  let base = Option.get !sample in
  let scalar = TScalar(TInteger(Unsigned U32)) in
  let rid = RegionId.of_int 99 in
  let free = TRef(RVar(Free rid),scalar,RShared) in
  let free_fn = TFnPtr {base with binder_value={base.binder_value with inputs=[free];output=scalar}} in
  if TypesUtils.ty_erase_regions free_fn <> TypesUtils.erase_regions_preserve_fn_binders free_fn
     || TypesUtils.ty_erase_regions (TypesUtils.ty_erase_regions free_fn) <> TypesUtils.ty_erase_regions free_fn then failwith "value erasure mismatch or not idempotent";
  if TypesUtils.ty_regions free_fn <> RegionId.Set.singleton rid
     || TypesUtils.ty_regions free <> RegionId.Set.singleton rid then failwith "free region tracking dropped";
  let outer = {Values.sv_id=Values.SymbolicValueId.of_int 0;sv_ty=free_fn} in
  if not (InterpUtils.symbolic_value_has_ended_regions (RegionId.Set.singleton rid) outer)
     || InterpUtils.symbolic_value_has_ended_regions RegionId.Set.empty outer then failwith "ended free region guard changed";
  let local = {outer with sv_ty=TFnPtr base} in
  if InterpUtils.symbolic_value_has_ended_regions (RegionId.Set.singleton rid) local then failwith "local region reported ended";
  if TypesUtils.ty_is_ety free_fn || not (TypesUtils.ty_is_ety (TypesUtils.ty_erase_regions free_fn)) then failwith "function ety free-region policy changed";
  if TypesUtils.ty_is_ety free || TypesUtils.ty_is_ety (TRef(RVar(Bound(0,RegionId.of_int 0)),scalar,RShared))
     || TypesUtils.ty_is_ety (TRef(RStatic,scalar,RShared)) then failwith "ordinary ety rejection relaxed";
  let rejects ty = try ignore (TypesUtils.ty_regions ty);false with Failure _ -> true in
  let make r = TFnPtr {base with binder_value={base.binder_value with inputs=[TRef(r,scalar,RShared)];output=scalar}} in
  List.iter (fun r -> if not (rejects (make r)) then failwith "invalid function region inventory accepted")
    [RVar(Bound(-1,rid));RVar(Bound(99,rid));RVar(Bound(0,rid));RErased;RBody (RegionId.of_int 0)];
  if not (rejects (TRef(RVar(Bound(0,RegionId.of_int 0)),scalar,RShared))) then failwith "ordinary bound region accepted";
  let nested depth = TFnPtr {base with binder_value={base.binder_value with
    inputs=[TFnPtr {binder_regions=[];binder_value={base.binder_value with
      inputs=[TRef(RVar(Bound(depth,RegionId.of_int 0)),scalar,RShared)];output=scalar}}];output=scalar}} in
  if not (RegionId.Set.is_empty (TypesUtils.ty_regions (nested 1))) || not (rejects (nested 0)) then failwith "empty nested inventory binder ignored";
  Printf.printf "%d actual function region inventories contain no free lifetimes; free/ended regions retained; value binder erasure preserved/idempotent; function ety scope checked; local scopes and invalid/ordinary/erased/body controls checked\n" !checked

let () =
  let c = match LlbcOfJson.crate_of_json_file Sys.argv.(1) with
    | Ok c -> c | Error e -> failwith e in
  let pointers = ref [] in
  let visitor = object
    inherit [_] LlbcAstUtils.iter_crate as super
    method! visit_ty () ty =
      (match ty with TFnPtr _ -> pointers := ty :: !pointers | _ -> ());
      super#visit_ty () ty
  end in
  visitor#visit_crate () c;
  let scalar = TScalar(TInteger(Unsigned U32)) in
  let analyse = TypesAnalysis.analyze_ty None TypeDeclId.Map.empty in
  let ref_ty kind ty = TRef(RVar(Free(RegionId.of_int 99)),ty,kind) in
  let pairs = ref 0 in
  List.iter (fun ty ->
    let before = show_ty ty in
    if analyse ty <> TypesAnalysis.type_borrows_info_init then failwith "signature borrows treated as pointer storage";
    List.iter (fun kind ->
      if analyse (ref_ty kind ty) <> analyse (ref_ty kind scalar) then failwith "outer pointer reference borrow erased";
      if analyse (TypesUtils.mk_tuple_ty [ref_ty kind scalar;ty]) <>
         analyse (TypesUtils.mk_tuple_ty [ref_ty kind scalar;scalar]) then failwith "prior tuple borrow erased";
      if analyse (TypesUtils.mk_tuple_ty [ty;ref_ty kind scalar]) <>
         analyse (TypesUtils.mk_tuple_ty [scalar;ref_ty kind scalar]) then failwith "later tuple borrow erased";
      incr pairs) [RShared;RMut];
    if analyse (ref_ty RMut (ref_ty RShared ty)) <>
       analyse (ref_ty RMut (ref_ty RShared scalar)) then failwith "nested outer borrow erased";
    if show_ty ty <> before then failwith "function signature mutated by borrow analysis") !pointers;
  Printf.printf "%d actual function pointers have no stored signature borrows; %d shared/mutable outer and adjacent contexts plus nested contexts retained\n" (List.length !pointers) !pairs
