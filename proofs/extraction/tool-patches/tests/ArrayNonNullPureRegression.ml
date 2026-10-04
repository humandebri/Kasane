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
      let failed = try ignore (SymbolicToPureTypes.translate_closed_rust_fnptr None sg); false
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
    let failed = try ignore (SymbolicToPureTypes.translate_closed_rust_fnptr None b); false
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

let () =
  let c = match LlbcOfJson.crate_of_json_file Sys.argv.(1) with
    | Ok c -> c | Error e -> failwith e in
  let casts = ref [] in
  let visitor = object
    inherit [_] LlbcAstUtils.iter_crate as super
    method! visit_CastTransmute () src dst =
      (match src,dst with TFnPtr _,TFnPtr _ -> casts := (src,dst) :: !casts | _ -> ());
      super#visit_CastTransmute () src dst
  end in
  visitor#visit_crate () c;
  if List.length !casts<>2 then failwith "actual function pointer transmute inventory changed";
  let supports = InterpExpressions.fnptr_transmute_representation_supported in
  let pure_cast src dst=SymbolicToPureExpressions.translate_rust_fnptr_transmute None
    (SymbolicToPureTypes.translate_sty None) src dst in
  let reject_pure src dst =
    let reached=ref false in
    let failed=try ignore(SymbolicToPureExpressions.translate_rust_fnptr_transmute None
      (fun ty -> reached:=true;SymbolicToPureTypes.translate_sty None ty) src dst);false
      with Errors.CFailure _ | Failure _ -> true in
    if not failed || !reached then failwith "unsupported Pure transmute reached type translation" in
  let checked = ref 0 in
  List.iter (fun (src,dst) ->
    if not (supports src dst) then failwith "actual function pointer representation rejected";
    let op=pure_cast src dst in
    let from,to_=match op with Pure.CastRustFnPtrTransmute(from,to_) -> from,to_ | _ -> failwith "transmute operation erased" in
    if from<>SymbolicToPureTypes.translate_sty None src || to_<>SymbolicToPureTypes.translate_sty None dst
       || op=Pure.CastRustFnItem(from,to_) then failwith "transmute endpoints/direction/kind lost";
    let subst : PureUtils.subst={ty_subst=(fun _ -> Pure.TLiteral(Pure.TUInt U32));
      cg_subst=(fun _ -> failwith "unexpected const capture");tr_subst=(fun _ -> failwith "unexpected trait capture");tr_self=Pure.Self} in
    let visitor=new PureUtils.subst_visitor in
    (match visitor#visit_cast_kind subst op with Pure.CastRustFnPtrTransmute(actual,target) ->
      if actual<>PureUtils.ty_substitute subst from || actual=from || target<>to_ then failwith "transmute source capture substitution invisible"
      | _ -> failwith "transmute kind changed by substitution");
    List.iter(fun backend -> Config.opt_backend:=Some backend;reject_pure src dst) [Config.Coq;Config.FStar;Config.HOL4];
    Config.opt_backend:=Some Config.Lean;
    let source = match src with TFnPtr t -> t | _ -> assert false in
    let target = match dst with TFnPtr t -> t | _ -> assert false in
    let scalar = TScalar(TInteger(Unsigned U32)) in
    let targets = [
      TFnPtr {target with binder_value={target.binder_value with abi=AbiC}};
      TFnPtr {target with binder_value={target.binder_value with is_variadic=true}};
      TFnPtr {target with binder_value={target.binder_value with is_unsafe=false}};
      TFnPtr {target with binder_value={target.binder_value with inputs=[]}};
      TFnPtr {target with binder_value={target.binder_value with output=scalar}};
      TFnPtr {target with binder_regions=[]};scalar] in
    List.iter (fun bad -> if supports src bad then failwith "unsupported function pointer target accepted";reject_pure src bad;incr checked) targets;
    List.iter (fun bad -> if supports bad dst then failwith "unsupported function pointer source accepted";reject_pure bad dst;incr checked)
      [TFnPtr {source with binder_value={source.binder_value with abi=AbiC}};
       TFnPtr {source with binder_value={source.binder_value with is_variadic=true}};
       TFnPtr {source with binder_regions=[]};scalar]) !casts;
  print_endline "two actual Pure transmute operators retain endpoints and kind; free source capture substitution visible; 22 representation negatives and three other backends rejected before type translation";
  Printf.printf "two actual function-pointer transmutes admitted only as retained representation casts; %d source/target controls rejected; call ABI/receiver remains unproved\n" !checked

let () =
  let c = match LlbcOfJson.crate_of_json_file Sys.argv.(1) with
    | Ok c -> c | Error e -> failwith e in
  let checked=ref 0 and sample=ref None in
  let visitor=object
    inherit [_] LlbcAstUtils.iter_crate as super
    method! visit_ty () ty =
      (match ty with TFnDef _ | TFnPtr _ ->
        if TypesUtils.ty_has_regions_in_set (RegionId.Set.singleton(RegionId.of_int 0)) ty then failwith "local bound matched free region set";
        if TypesUtils.ty_has_free_regions ty then failwith "local bound treated as free";
        if TypesUtils.ty_has_regions_in_pred (function RVar(Bound _) -> failwith "local region reached free predicate" | _ -> false) ty then failwith "unexpected free predicate result";
        incr checked;(match ty with TFnPtr t -> sample := Some t | _ -> ())
       | _ -> ());super#visit_ty () ty
  end in
  visitor#visit_crate () c;
  let base=Option.get !sample in
  let scalar=TScalar(TInteger(Unsigned U32)) in
  let rid=RegionId.of_int 99 in
  let make r=TFnPtr {base with binder_value={base.binder_value with inputs=[TRef(r,scalar,RShared)];output=scalar}} in
  let free=make(RVar(Free rid)) in
  if not(TypesUtils.ty_has_regions_in_set (RegionId.Set.singleton rid) free) || not(TypesUtils.ty_has_free_regions free)
     || TypesUtils.ty_has_regions_in_set RegionId.Set.empty free then failwith "free region predicate lost";
  if not(TypesUtils.ty_has_erased_regions (make RErased)) then failwith "erased predicate changed";
  List.iter(fun ty ->
    let failed=try ignore(TypesUtils.ty_has_regions_in_pred (fun _ -> false) ty);false with Failure _ -> true in
    if not failed then failwith "bad region scope accepted by predicate")
    [make(RVar(Bound(-1,rid)));make(RVar(Bound(99,rid)));make(RVar(Bound(0,rid)))];
  Printf.printf "%d actual function region predicates preserve local/free distinction; free set and erased controls retained; invalid local scopes rejected\n" !checked

let () =
  let c=match LlbcOfJson.crate_of_json_file Sys.argv.(1) with Ok c -> c | Error e -> failwith e in
  let items=ref [] in
  let visitor=object
    inherit [_] LlbcAstUtils.iter_crate as super
    method! visit_ty () ty =
      (match ty with TFnDef f -> items:=f::!items | _ -> ());super#visit_ty () ty
  end in
  visitor#visit_crate () c;
  if List.length !items<>4 then failwith "actual function-item inventory changed";
  let roundtrip f =
    let m,args=match SymbolicToPureTypes.split_open_trait_fn_item f with Some x -> x | None -> failwith "actual trait item rejected" in
    let tr=match args.trait_refs with [tr] -> tr | _ -> failwith "trait evidence missing" in
    if args.types<>tr.trait_decl_ref.binder_value.generics.types || args.const_generics<>[] || args.regions<>[] then failwith "Self/evidence captures not explicit";
    let reconstructed : fn_ptr region_binder = {
      binder_regions=m.item_regions;
      binder_value={kind=TraitMethod(tr,m.item_method_id);
        generics={TypesUtils.empty_generic_args with regions=m.item_region_args}}} in
    if reconstructed<>f || tr.trait_decl_ref.binder_value.id<>m.item_trait_id then failwith "source identity/binder roundtrip failed";
    m,args in
  let rebinding=ref 0 and rejected_shapes=ref 0 in
  List.iter (fun f ->
    let m,_=roundtrip f in
    let g=f.binder_value.generics in
    let tr,method_id=match f.binder_value.kind with TraitMethod(tr,id) -> tr,id | _ -> failwith "expected trait method" in
    let pred=tr.trait_decl_ref.binder_value in
    let with_tr tr={f with binder_value={f.binder_value with kind=TraitMethod(tr,method_id)}} in
    let with_regions regions={f with binder_value={f.binder_value with generics={g with regions}}} in
    let with_self ty=with_tr {tr with trait_decl_ref={tr.trait_decl_ref with binder_value={pred with generics={pred.generics with types=[ty]}}}} in
    let bad=[
      with_tr {tr with kind=Clause(Bound(0,TraitClauseId.of_int 0))};
      with_tr {tr with kind=Self};
      with_tr {tr with trait_decl_ref={tr.trait_decl_ref with binder_regions=f.binder_regions}};
      with_self (TScalar(TInteger(Unsigned U32)));
      with_self (TVar(Bound(0,TypeVarId.of_int 0)));
      with_tr {tr with trait_decl_ref={tr.trait_decl_ref with binder_value={pred with generics={pred.generics with types=pred.generics.types@pred.generics.types}}}};
      with_tr {tr with trait_decl_ref={tr.trait_decl_ref with binder_value={pred with generics={pred.generics with regions=[RStatic]}}}};
      with_tr {tr with trait_decl_ref={tr.trait_decl_ref with binder_value={pred with generics={pred.generics with trait_refs=[tr]}}}};
      {f with binder_value={f.binder_value with generics={g with types=[TScalar(TInteger(Unsigned U32))]}}};
      {f with binder_value={f.binder_value with generics={g with trait_refs=[tr]}}};
      {f with binder_regions=[]};
      with_regions [RErased];
      with_regions [RVar(Free(RegionId.of_int 0))];
      with_regions [RVar(Bound(1,RegionId.of_int 0))];
      with_regions [RVar(Bound(0,RegionId.of_int 99))]] in
    List.iter(fun bad -> if SymbolicToPureTypes.split_open_trait_fn_item bad<>None then failwith "unsupported item shape accepted";incr rejected_shapes) bad;
    let ty=SymbolicToPureTypes.translate_sty None (TFnDef f) in
    let p,args=match ty with Pure.TAdt(Pure.TBuiltin(Pure.TRustTraitFnItem p),args) -> p,args | _ -> failwith "item lowered to arrow/unit" in
    if p<>m || List.length args.types<>1 || List.length args.trait_refs<>1 || args.const_generics<>[] then failwith "Pure template captures lost";
    let scalar=Pure.TLiteral(Pure.TUInt U32) in
    let subst : PureUtils.subst = {
      ty_subst=(fun _ -> scalar);
      cg_subst=(fun _ -> failwith "unexpected const variable");
      tr_subst=(fun _ -> Pure.Clause(Free(TraitClauseId.of_int 77)));
      tr_self=Pure.Self} in
    let substituted=PureUtils.ty_substitute subst ty in
    (match substituted with
     | Pure.TAdt(Pure.TBuiltin(Pure.TRustTraitFnItem actual),g) ->
       if actual<>m || g.types<>[scalar] then failwith "metadata changed or Self substitution invisible";
       (match g.trait_refs with
        | [{Pure.trait_id=Pure.Clause(Free id);trait_decl_ref}] ->
          if id<>TraitClauseId.of_int 77 || trait_decl_ref.decl_generics.types<>[scalar] then failwith "trait evidence substitution invisible"
        | _ -> failwith "trait capture lost")
     | _ -> failwith "item marker changed by substitution");
    List.iter (fun index ->
      let sb={Substitute.empty_subst with
        ty_subst=(function Free _ -> TVar(Free(TypeVarId.of_int index)) | var -> TVar var);
        tr_subst=(function Free _ -> Clause(Free(TraitClauseId.of_int(index+1))) | var -> Clause var)} in
      let changed=Substitute.st_substitute_visitor#visit_region_binder
        Substitute.st_substitute_visitor#visit_fn_ptr sb f in
      let actual,_=roundtrip changed in
      if actual<>m then failwith "free variable rebinding leaked into opaque metadata";
      incr rebinding) [0;1;7;31;139];
    List.iter(fun backend -> Config.opt_backend:=Some backend;
      let rejected=try ignore(SymbolicToPureTypes.translate_sty None (TFnDef f));false with Errors.CFailure _ | Failure _ -> true in
      if not rejected then failwith "unsupported backend accepted item template") [Config.Coq;Config.FStar;Config.HOL4];
    Config.opt_backend:=Some Config.Lean) !items;
  Printf.printf "four actual late-bound trait items retain source identity/binders; Self and trait evidence visible to Pure substitution; %d free-variable rebinding roundtrips; three non-Lean backends rejected; %d unsupported item shapes rejected\n" !rebinding !rejected_shapes

let () =
  let src=Pure.TVar(Free(TypeVarId.of_int 11)) and dst=Pure.TVar(Free(TypeVarId.of_int 12)) in
  let scalar=Pure.TLiteral(Pure.TUInt U32) and boolean=Pure.TLiteral Pure.TBool in
  let subst : PureUtils.subst = {
    ty_subst=(fun id -> if id=TypeVarId.of_int 11 then scalar else if id=TypeVarId.of_int 12 then boolean else failwith "unexpected endpoint variable");
    cg_subst=(fun _ -> failwith "unexpected const capture");
    tr_subst=(fun _ -> failwith "unexpected trait capture");tr_self=Pure.Self} in
  let visitor=new PureUtils.subst_visitor in
  let cast=Pure.CastRustFnItem(src,dst) in
  if visitor#visit_cast_kind subst cast<>Pure.CastRustFnItem(scalar,boolean) then failwith "cast endpoint substitution invisible";
  if visitor#visit_cast_kind subst (Pure.CastRustFnItem(dst,src))<>Pure.CastRustFnItem(boolean,scalar) then failwith "cast direction lost";
  print_endline "retained Pure coercion visits both endpoint type variables and preserves direction; synthetic operator traversal only; source pipeline tested separately"

let () =
  let c=match LlbcOfJson.crate_of_json_file Sys.argv.(1) with Ok c -> c | Error e -> failwith e in
  let templates=ref [] in
  let visitor=object
    inherit [_] LlbcAstUtils.iter_crate as super
    method! visit_ty () ty = (match ty with TFnPtr s when not(SymbolicToPureTypes.supported_closed_rust_fnptr s) -> templates:=s::!templates | _ -> ());super#visit_ty () ty
  end in
  visitor#visit_crate () c;
  let instantiate signature captures =
    let mapper=object
      inherit [_] Types.map_ty as super
      method! visit_ty () ty=match ty with
        | TVar(Bound(0,id)) -> List.nth captures (TypeVarId.to_int id)
        | TVar _ -> failwith "unowned variable in template metadata"
        | _ -> super#visit_ty () ty
    end in mapper#visit_region_binder mapper#visit_fun_sig () signature in
  let rejected=ref 0 and rebound=ref 0 in
  List.iter(fun s ->
    let m,captures=match SymbolicToPureTypes.split_rust_fnptr_template s with Some x -> x | None -> failwith "actual open pointer template rejected" in
    if m.template_type_slots<>List.length captures || instantiate m.template_signature captures<>s then failwith "source pointer reconstruction failed";
    let ty=SymbolicToPureTypes.translate_sty None (TFnPtr s) in
    let expected=List.map(SymbolicToPureTypes.translate_sty None) captures in
    (match ty with Pure.TAdt(Pure.TBuiltin(Pure.TRustFnPtrTemplate actual),g) when actual=m && g.types=expected && g.const_generics=[] && g.trait_refs=[] -> () | _ -> failwith "pointer captures lost or lowered to arrow");
    let scalar=Pure.TLiteral(Pure.TUInt U32) in
    let subst : PureUtils.subst={ty_subst=(fun _ -> scalar);cg_subst=(fun _ -> failwith "const capture");tr_subst=(fun _ -> failwith "trait capture");tr_self=Pure.Self} in
    let g=match PureUtils.ty_substitute subst ty with Pure.TAdt(Pure.TBuiltin(Pure.TRustFnPtrTemplate actual),g) when actual=m -> g | _ -> failwith "template metadata changed on substitution" in
    if g.types<>List.map(fun _ -> scalar) captures then failwith "template type substitution invisible";
    let raw_scalar=TScalar(TInteger(Unsigned U32)) in
    let mapper=object inherit [_] Types.map_ty as super
      method! visit_ty () ty=match ty with TVar(Free _) -> raw_scalar | _ -> super#visit_ty () ty end in
    if instantiate m.template_signature (List.map(fun _ -> raw_scalar) captures)<>mapper#visit_region_binder mapper#visit_fun_sig () s then failwith "raw substitution reconstruction mismatch";
    List.iter(fun offset ->
      let renamer=object inherit [_] Types.map_ty as super
        method! visit_ty () ty=match ty with TVar(Free id) -> TVar(Free(TypeVarId.of_int(TypeVarId.to_int id+offset))) | _ -> super#visit_ty () ty end in
      let renamed=renamer#visit_region_binder renamer#visit_fun_sig () s in
      let actual,args=match SymbolicToPureTypes.split_rust_fnptr_template renamed with Some x -> x | None -> failwith "renamed template rejected" in
      if actual<>m || instantiate actual.template_signature args<>renamed then failwith "template metadata depends on source free variable IDs";
      incr rebound) [1;7;31;139];
    let sg=s.binder_value in
    let input ty={s with binder_value={sg with inputs=[ty]}} in
    let ref_input r=input(TRef(r,TVar(Free(TypeVarId.of_int 0)),RShared)) in
    let first=List.hd s.binder_regions in
    List.iter(fun bad -> if SymbolicToPureTypes.split_rust_fnptr_template bad<>None then failwith "unsupported template accepted";incr rejected)
      [{s with binder_value={sg with abi=AbiC}};
       {s with binder_value={sg with is_variadic=true}};
       {s with binder_regions=[]};{s with binder_regions=[first;first]};
       ref_input RErased;ref_input(RVar(Free(RegionId.of_int 0)));
       ref_input(RVar(Bound(1,RegionId.of_int 0)));
       ref_input(RVar(Bound(0,RegionId.of_int 99)));
       input(TVar(Bound(0,TypeVarId.of_int 0)));
       input(TVar(Free(TypeVarId.of_int (-1))));input(TFnPtr s);
       input(TSlice(TVar(Free(TypeVarId.of_int 0)),None))]) !templates;
  let base=List.hd !templates in
  let two={base with binder_value={base.binder_value with
    inputs=[TVar(Free(TypeVarId.of_int 9));TVar(Free(TypeVarId.of_int 2));TVar(Free(TypeVarId.of_int 9))];
    output=TVar(Free(TypeVarId.of_int 2))}} in
  let m,captures=match SymbolicToPureTypes.split_rust_fnptr_template two with Some x -> x | None -> failwith "two-variable template rejected" in
  if m.template_type_slots<>2 || captures<>[TVar(Free(TypeVarId.of_int 2));TVar(Free(TypeVarId.of_int 9))]
     || instantiate m.template_signature captures<>two || instantiate m.template_signature (List.rev captures)=two then failwith "distinct or repeated slots conflated";
  Printf.printf "template metadata invariant under %d injective free-variable renamings; two-variable repeated/distinct slot control retained\n" !rebound;
  Printf.printf "%d actual open pointer templates reconstruct exact source; Pure capture substitution and raw reconstruction commute; %d unsupported template shapes rejected\n" (List.length !templates) !rejected

let () =
  let c=match LlbcOfJson.crate_of_json_file Sys.argv.(1) with Ok c -> c | Error e -> failwith e in
  let types=ref [] and values=ref [] in
  let visitor=object
    inherit [_] LlbcAstUtils.iter_crate as super
    method! visit_ty () ty=(match ty with TFnDef f -> types:=f::!types | _ -> ());super#visit_ty () ty
    method! visit_CFnDef () ptr=values:=ptr::!values;super#visit_CFnDef () ptr
  end in visitor#visit_crate () c;
  if List.length !values<>2 then failwith "actual source function-item constants changed";
  let prepared_values=ref [] in
  let prepared_visitor=object inherit [_] LlbcAstUtils.iter_crate as super
    method! visit_CFnDef () ptr=prepared_values:=ptr::!prepared_values;super#visit_CFnDef () ptr end in
  let prepared={c with fun_decls=LlbcAst.FunDeclId.Map.map(PrePasses.erase_body_regions c) c.fun_decls} in
  prepared_visitor#visit_crate () prepared;
  if List.length !prepared_values<>2 then failwith "prepass source constant inventory changed";
  let rejected=ref 0 and type_rejected=ref 0 in
  List.iter(fun (raw_value : fn_ptr) ->
    let source=List.find(fun (f : fn_ptr region_binder) -> f.binder_value.kind=raw_value.kind) !types in
    let raw_ty=TFnDef source in
    let value=List.find(fun (ptr : fn_ptr) -> ptr.kind=raw_value.kind) !prepared_values in
    let expected=SymbolicToPureTypes.translate_sty None raw_ty in
    let make value expected=SymbolicToPureExpressions.translate_rust_trait_fn_item_value None c raw_ty value expected in
    let expr=make value expected in
    let m,regions,g=match expr.e with Pure.Qualif{id=Pure.RustTraitFnItemValue(m,regions);generics} -> m,regions,generics | _ -> failwith "trait item value lowered to arrow or lost identity" in
    if expr.ty<>expected || regions<>value.generics.regions then failwith "source type/value region distinction lost";
    if not(PureTypeCheck.rust_trait_fn_item_value_type_matches c m regions g expr.ty) then failwith "valid value rejected by type check";
    let subst : PureUtils.subst={ty_subst=(fun _ -> Pure.TLiteral(Pure.TUInt U32));cg_subst=(fun _ -> failwith "unexpected const capture");tr_subst=(fun _ -> Pure.Clause(Free(TraitClauseId.of_int 77)));tr_self=Pure.Self} in
    let mapper=new PureUtils.subst_visitor in
    let substituted=mapper#visit_texpr subst expr in
    (match substituted.e with Pure.Qualif{id=Pure.RustTraitFnItemValue(actual,rs);generics=gs} ->
       if actual<>m || rs<>regions || gs.types<>[Pure.TLiteral(Pure.TUInt U32)]
          || not(PureTypeCheck.rust_trait_fn_item_value_type_matches c actual rs gs substituted.ty) then failwith "value capture substitution not type consistent"
       | _ -> failwith "value identity changed by substitution");
    let other=List.find(fun (p : fn_ptr) -> p.kind<>raw_value.kind) !values in
    List.iter(fun bad ->
      let failed=try ignore(make bad expected);false with Errors.CFailure _ | Failure _ -> true in
      if not failed then failwith "wrong source value accepted";incr rejected)
      [{value with kind=other.kind};
       {value with generics={value.generics with regions=[]}};
       {value with generics={value.generics with regions=[RStatic]}};
       {value with generics={value.generics with regions=[RBody(RegionId.of_int 12)]}};
       {value with generics={value.generics with regions=[RVar(Bound(0,RegionId.of_int 0))]}};
       {value with generics={value.generics with types=[TScalar(TInteger(Unsigned U32))]}}];
    let wrong_type=try ignore(make value (Pure.TLiteral Pure.TBool));false with Errors.CFailure _ | Failure _ -> true in
    if not wrong_type then failwith "wrong expected type accepted";
    let typed (mm : Pure.rust_trait_fn_item) rs (gs : Pure.generic_args) =
      (mm,rs,gs,Pure.TAdt(Pure.TBuiltin(Pure.TRustTraitFnItem mm),gs)) in
    let tr=List.hd g.trait_refs in
    List.iter(fun (mm,rs,gs,ty) ->
      if PureTypeCheck.rust_trait_fn_item_value_type_matches c mm rs gs ty then failwith "malformed qualifier accepted";incr type_rejected)
      [(m,regions,g,Pure.TLiteral Pure.TBool);
       typed m [] g;
       typed {m with item_regions=[]} regions g;
       typed {m with item_method_id=TraitMethodId.of_int 999} regions g;
       typed m regions {g with types=[]};
       typed m regions {g with trait_refs=[]};
       typed m regions {g with trait_refs=g.trait_refs@g.trait_refs};
       typed m regions {g with types=[Pure.TLiteral Pure.TBool]};
       typed m regions {g with trait_refs=[{tr with trait_decl_ref={tr.trait_decl_ref with trait_decl_id=TraitDeclId.of_int 999}}]}];
    List.iter(fun backend -> Config.opt_backend:=Some backend;
      let failed=try ignore(make value expected);false with Errors.CFailure _ | Failure _ -> true in
      if not failed then failwith "non-Lean value accepted") [Config.Coq;Config.FStar;Config.HOL4];
    Config.opt_backend:=Some Config.Lean) !values;
  Printf.printf "two actual trait item constants retain identity/type binders/erased value regions; Self and evidence substitution remain type consistent; %d value and %d type controls rejected, wrong expected types and three backends rejected\n" !rejected !type_rejected

let () =
  let c=match LlbcOfJson.crate_of_json_file Sys.argv.(1) with Ok c -> c | Error e -> failwith e in
  if not(NonNullSourceOptions.nonnull_source_options c.options) then failwith "expanded source options not exact";
  let original={c.options with included=List.filter((<>) "core::ptr::non_null::NonNull") c.options.included} in
  if not(FormattingSourceOptions.formatting_source_options original) then failwith "source profile changed beyond NonNull include";
  List.iter(fun options -> if NonNullSourceOptions.nonnull_source_options options then failwith "weakened NonNull source options accepted")
    [{c.options with skip_borrowck=true};{c.options with no_typecheck=true};
     {c.options with no_normalize=true};{c.options with erase_body_lifetimes=true};
     {c.options with reconstruct_panic_calls=true};{c.options with no_compute_layout_guarantees=true};
     {c.options with included=c.options.included@["unknown"]}];
  let decl=TypeDeclId.Map.bindings c.type_decls |> List.map snd |> List.find(fun (d : type_decl) ->
    List.filter_map(function PeIdent(n,_) -> Some n | _ -> None) d.item_meta.name=["core";"ptr";"non_null";"NonNull"]) in
  let base=match decl.kind with Struct[{field_ty=TPattern(base,NotNull);_}] -> base | _ -> failwith "actual NonNull field shape changed" in
  let source=TPattern(base,NotNull) in
  let translated=SymbolicToPureTypes.translate_sty None source in
  let pointee,kind=match base with TRawPtr(t,k) -> t,k | _ -> failwith "source pointer base missing" in
  (match translated with Pure.TAdt(Pure.TBuiltin(Pure.TRustNotNullPtr Pure.Const),g) ->
    if g.types<>[SymbolicToPureTypes.translate_sty None pointee] || g.const_generics<>[] || g.trait_refs<>[] then failwith "NotNull pointee capture lost"
    | _ -> failwith "non-null constraint erased");
  let subst : PureUtils.subst={ty_subst=(fun _ -> Pure.TLiteral(Pure.TUInt U32));cg_subst=(fun _ -> failwith "const capture");tr_subst=(fun _ -> failwith "trait capture");tr_self=Pure.Self} in
  (match PureUtils.ty_substitute subst translated with Pure.TAdt(Pure.TBuiltin(Pure.TRustNotNullPtr Pure.Const),g) when g.types=[Pure.TLiteral(Pure.TUInt U32)] -> () | _ -> failwith "pointee substitution invisible");
  if translated=SymbolicToPureTypes.translate_sty None base
     || translated=SymbolicToPureTypes.translate_sty None (TPattern(TRawPtr(pointee,RMut),NotNull)) then failwith "non-null/mutability distinctions lost";
  let scalar=TScalar(TInteger(Unsigned U32)) in
  let lo={kind=CInteger(UnsignedInteger(U32,Z.zero));ty=scalar}
  and hi={kind=CInteger(UnsignedInteger(U32,Z.one));ty=scalar} in
  let bad=[TPattern(scalar,NotNull);TPattern(TVar(Free(TypeVarId.of_int 0)),NotNull);
    TPattern(TNever,NotNull);TPattern(TRef(RVar(Free(RegionId.of_int 99)),scalar,RShared),NotNull);
    TPattern(base,Range(lo,hi));TPattern(source,NotNull)] in
  List.iter(fun ty ->
    let b,p=match ty with TPattern(b,p) -> b,p | _ -> assert false in
    if TypesAnalysis.supported_notnull_rawptr_pattern b p then failwith "unsupported NotNull shape accepted";
    let failed=try ignore(SymbolicToPureTypes.translate_sty None ty);false with Errors.CFailure _ | Failure _ -> true in
    if not failed then failwith "unsupported NotNull shape translated") bad;
  let analyse=TypesAnalysis.analyze_ty None TypeDeclId.Map.empty in
  let stored=TPattern(TRawPtr(TRef(RVar(Free(RegionId.of_int 99)),scalar,RShared),kind),NotNull) in
  if analyse source<>TypesAnalysis.type_borrows_info_init || analyse stored<>TypesAnalysis.type_borrows_info_init then failwith "pointee borrow treated as pointer storage";
  let ref_ty kind t=TRef(RVar(Free(RegionId.of_int 111)),t,kind) in
  List.iter(fun k ->
    if analyse(ref_ty k stored)<>analyse(ref_ty k scalar)
       || analyse(TypesUtils.mk_tuple_ty [ref_ty k scalar;stored])<>analyse(TypesUtils.mk_tuple_ty [ref_ty k scalar;scalar])
       || analyse(TypesUtils.mk_tuple_ty [stored;ref_ty k scalar])<>analyse(TypesUtils.mk_tuple_ty [scalar;ref_ty k scalar]) then failwith "outer or adjacent borrows lost") [RShared;RMut];
  if TypesUtils.ty_regions stored<>RegionId.Set.singleton(RegionId.of_int 99) || not(TypesUtils.ty_has_free_regions stored) then failwith "pointer metadata free-region tracking lost";
  let compared=ref false in
  let compare lhs rhs=compared:=true;if lhs<>pointee || rhs<>pointee then failwith "pointee comparison structure erased";true in
  if InterpBorrowsCore.compare_notnull_pointer_pointees None compare source source<>Some true || not !compared then failwith "pointer comparison callback omitted";
  let free=ref false in
  let callback lhs rhs=free:=TypesUtils.ty_regions lhs=RegionId.Set.singleton(RegionId.of_int 99) && TypesUtils.ty_regions rhs=RegionId.Set.singleton(RegionId.of_int 99);false in
  if InterpBorrowsCore.compare_notnull_pointer_pointees None callback stored stored<>Some false || not !free then failwith "pointee regions/comparison result erased";
  let mismatch_reached=ref false in
  let rejected=try ignore(InterpBorrowsCore.compare_notnull_pointer_pointees None (fun _ _ -> mismatch_reached:=true;true) source (TPattern(TRawPtr(pointee,RMut),NotNull)));false with Errors.CFailure _ | Failure _ -> true in
  if not rejected || !mismatch_reached || InterpBorrowsCore.compare_notnull_pointer_pointees None compare base base<>None then failwith "comparison shape or pointer kind relaxed";
  List.iter(fun backend -> Config.opt_backend:=Some backend;
    let failed=try ignore(SymbolicToPureTypes.translate_sty None source);false with Errors.CFailure _ | Failure _ -> true in
    if not failed then failwith "non-Lean pointer marker accepted") [Config.Coq;Config.FStar;Config.HOL4];Config.opt_backend:=Some Config.Lean;
  print_endline "actual NonNull field retains constraint/mutability/visible pointee capture; six malformed shapes and three backends rejected; no stored pointee borrows, outer/adjacent flags and free-region inventory preserved; comparison traverses complete pointee and retains callback result; seven weakened options rejected"

let () =
  let c=match LlbcOfJson.crate_of_json_file Sys.argv.(1) with Ok c -> c | Error e -> failwith e in
  (* Match runtime cast types after the existing body-region erasure pass. *)
  let c={c with fun_decls=LlbcAst.FunDeclId.Map.map(PrePasses.erase_body_regions c) c.fun_decls} in
  let casts=ref [] in
  let visitor=object
    inherit [_] LlbcAstUtils.iter_crate as super
    method! visit_CastTransmute () src dst =
      (match src with TRef(_,TArray _,RShared) -> casts:=(src,dst)::!casts | _ -> ());
      super#visit_CastTransmute () src dst
  end in
  visitor#visit_crate () c;
  if List.length !casts<>2 then failwith "actual array transmute inventory changed";
  let guard=InterpExpressions.array_ref_nonnull_representation_supported in
  let rejected=ref 0 in
  List.iter(fun (src,dst) ->
    if not(guard c src dst) then failwith ("actual array to NonNull representation rejected: "^show_ty src^" -> "^show_ty dst);
    let region,elem,len,r=match src,dst with
      TRef(region,TArray(elem,len,None),RShared),TAdt r -> region,elem,len,r | _ -> assert false in
    let zero={kind=CInteger(UnsignedInteger(Usize,Z.zero));ty=len.ty} in
    if not(guard c (TRef(region,TArray(elem,zero,None),RShared)) dst)
       || not(guard c (TRef(RErased,TArray(elem,len,None),RShared)) dst) then failwith "empty length or erased body region rejected";
    let bad=[TRef(region,TArray(elem,len,None),RMut),dst;
      TRef(region,TArray(elem,{len with ty=TScalar(TInteger(Unsigned U32))},None),RShared),dst;
      src,TAdt{r with generics={r.generics with types=[TNever]}};
      src,TAdt{r with generics={r.generics with types=[]}};
      src,TAdt{r with generics={r.generics with types=[elem;elem]}};
      src,TAdt{r with generics={r.generics with regions=[RStatic]}};
      src,TAdt{r with generics={r.generics with const_generics=[zero]}};
      src,TAdt{r with id=TypeDeclId.of_int 999};
      TRef(region,TArray(TVar(Free(TypeVarId.of_int 0)),len,None),RShared),TAdt{r with generics={r.generics with types=[TVar(Free(TypeVarId.of_int 0))]}}] in
    List.iter(fun (a,b) -> if guard c a b then failwith "malformed array representation accepted";incr rejected) bad;
    let d=TypeDeclId.Map.find r.id c.type_decls in
    let field=match d.kind with Struct[f] -> f | _ -> assert false in
    let target,l=match d.layout with [pair] -> pair | _ -> assert false in
    let mutants=[{d with kind=Opaque};{d with kind=Struct[]};{d with kind=Struct[field;field]};
      {d with kind=Struct[{field with field_ty=TRawPtr(TVar(Free(TypeVarId.of_int 0)),RShared)}]};
      {d with kind=Struct[{field with field_ty=TPattern(TRawPtr(TVar(Free(TypeVarId.of_int 0)),RMut),NotNull)}]};
      {d with item_meta={d.item_meta with name=[]}};
      {d with generics={d.generics with types=[]}};
      {d with ptr_metadata=Length};{d with layout=[]};
      {d with layout=[target,{l with repr={l.repr with transparent=false}}]};
      {d with layout=[target,{l with size={l.size with guarantee=None}}]};
      {d with layout=[target,{l with align={l.align with guarantee=None}}]};
      {d with layout=["unknown-target",l]};
      {d with layout=[target,{l with variant_layouts=[]}]}] in
    List.iter(fun mutant ->
      let crate={c with type_decls=TypeDeclId.Map.add r.id mutant c.type_decls} in
      if guard crate src dst then failwith "malformed NonNull declaration or layout accepted";
      incr rejected) mutants) !casts;
  Printf.printf "two actual array-reference to NonNull casts matched fixed transparent layout; empty arrays and erased regions retained without dereference; %d type/declaration/layout controls rejected; pointer value extraction remains unproved\n" !rejected

let () =
  let raw=match LlbcOfJson.crate_of_json_file Sys.argv.(1) with Ok c -> c | Error e -> failwith e in
  let c={raw with fun_decls=LlbcAst.FunDeclId.Map.map(PrePasses.erase_body_regions raw) raw.fun_decls} in
  let casts=ref [] in
  let visitor=object
    inherit [_] LlbcAstUtils.iter_crate as super
    method! visit_CastTransmute () src dst =
      (match src with TRef(_,TArray _,RShared) -> casts:=(src,dst)::!casts | _ -> ());
      super#visit_CastTransmute () src dst
  end in
  visitor#visit_crate () c;
  let restore=object inherit [_] map_ty method! visit_RErased _ = RVar(Free(RegionId.of_int 99)) end in
  let count=ref 0 in
  List.iter(fun (src,dst) ->
    let array=match src with TRef(_,a,RShared) -> a | _ -> assert false in
    let full=restore#visit_ty () array in
    let loan : Values.symbolic_value={sv_id=Values.SymbolicValueId.of_int 77;sv_ty=full} in
    let result=match InterpExpressions.array_ref_nonnull_symbolic_type c src dst loan with
      Some t -> t | None -> failwith "matching full-region loan rejected" in
    let elem=match full with TArray(t,_,None) -> t | _ -> assert false in
    let expected=match dst with TAdt r -> TAdt{r with generics={r.generics with types=[elem]}} | _ -> assert false in
    if result<>expected || TypesUtils.ty_erase_regions result<>dst
       || TypesUtils.ty_regions result<>TypesUtils.ty_regions full then failwith "pointee region metadata not retained";
    let output={loan with sv_ty=result} in
    let ended=RegionId.Set.singleton(RegionId.of_int 99) in
    if InterpUtils.symbolic_value_has_ended_regions ended output<>
       (not(RegionId.Set.is_empty(TypesUtils.ty_regions full))) then failwith "ended pointee region bypassed";
    if InterpUtils.symbolic_value_has_ended_regions (RegionId.Set.singleton(RegionId.of_int 100)) output then failwith "unrelated ended region leaked";
    let len=match full with TArray(_,n,None) -> n | _ -> assert false in
    let zero={kind=CInteger(UnsignedInteger(Usize,Z.zero));ty=len.ty} in
    let bad=[TNever;elem;TArray(TNever,len,None);TArray(elem,zero,None);
      TRef(RVar(Free(RegionId.of_int 99)),full,RShared);
      TArray(TRef(RErased,TNever,RShared),len,None)] in
    List.iter(fun ty ->
      if InterpExpressions.array_ref_nonnull_symbolic_type c src dst {loan with sv_ty=ty}<>None then failwith "incompatible symbolic loan accepted";
      incr count) bad;
    if TypesUtils.ty_has_free_regions full then (
      if InterpExpressions.array_ref_nonnull_symbolic_type c src dst {loan with sv_ty=array}<>None then failwith "erased pointee regions accepted as symbolic type";
      incr count)) !casts;
  Printf.printf "two actual casts recover exact pointee-region metadata from matching symbolic array loans; erased result matches destination and ended/free region tracking retained; %d incompatible loan controls rejected; pointer address/provenance extraction remains unproved\n" !count

let () =
  let raw=match LlbcOfJson.crate_of_json_file Sys.argv.(1) with Ok c -> c | Error e -> failwith e in
  let c={raw with fun_decls=LlbcAst.FunDeclId.Map.map(PrePasses.erase_body_regions raw) raw.fun_decls} in
  let casts=ref [] in
  let visitor=object
    inherit [_] LlbcAstUtils.iter_crate as super
    method! visit_CastTransmute () src dst =
      (match src with TRef(_,TArray _,RShared) -> casts:=(src,dst)::!casts | _ -> ());
      super#visit_CastTransmute () src dst
  end in visitor#visit_crate () c;
  let restore=object inherit [_] map_ty method! visit_RErased _ = RVar(Free(RegionId.of_int 99)) end in
  let rejected=ref 0 in
  List.iter(fun (src,dst) ->
    let full=restore#visit_ty () (match src with TRef(_,a,RShared) -> a | _ -> assert false) in
    let sv : Values.symbolic_value={sv_id=Values.SymbolicValueId.of_int 77;sv_ty=full} in
    let loan : Values.tvalue={value=Values.VSymbolic sv;ty=TypesUtils.ty_erase_regions full} in
    let bid=Values.BorrowId.of_int 11 and sid=Values.SharedBorrowId.of_int 22 in
    let operand : Values.tvalue={value=Values.VBorrow(Values.VSharedBorrow(bid,sid));ty=src} in
    let dest=Option.get(InterpExpressions.array_ref_nonnull_symbolic_type c src dst sv) in
    let translate=SymbolicToPureTypes.translate_sty None in
    let factory=SymbolicToPureExpressions.translate_rust_shared_array_nonnull None c in
    let op=factory translate (fun b -> if b<>bid then failwith "wrong actual origin lookup";loan) src dst operand dest in
    let origin,from,to_=match op with Pure.CastRustSharedArrayNonNull(o,a,b) -> o,a,b | _ -> failwith "retained array conversion kind erased" in
    if origin.origin_borrow<>bid || origin.origin_shared_borrow<>sid || origin.origin_loan_symbolic<>sv.sv_id
       || from<>translate src || to_<>translate dest then failwith "origin or endpoints lost";
    let other={operand with value=Values.VBorrow(Values.VSharedBorrow(Values.BorrowId.of_int 12,sid))} in
    if factory translate (fun _ -> loan) src dst other dest=op then failwith "distinct shared origins collapsed";
    let alias={operand with value=Values.VBorrow(Values.VSharedBorrow(bid,Values.SharedBorrowId.of_int 23))} in
    (match factory translate (fun _ -> loan) src dst alias dest with
      Pure.CastRustSharedArrayNonNull(o,a,b) ->
        if o.origin_borrow<>origin.origin_borrow || o.origin_loan_symbolic<>origin.origin_loan_symbolic
           || o.origin_shared_borrow=origin.origin_shared_borrow || a<>from || b<>to_ then failwith "same-loan alias origin lost"
      | _ -> failwith "alias conversion kind erased");
    let subst : PureUtils.subst={ty_subst=(fun id -> Pure.TVar(Free id));
      cg_subst=(fun _ -> Pure.CgValue(Pure.VScalar(UnsignedInteger(Usize,Z.of_int 3))));
      tr_subst=(fun id -> Pure.Clause(Free id));tr_self=Pure.Self} in
    let visitor=new PureUtils.subst_visitor in
    (match visitor#visit_cast_kind subst op with Pure.CastRustSharedArrayNonNull(o,a,b) ->
      if o<>origin || a<>PureUtils.ty_substitute subst from || b<>PureUtils.ty_substitute subst to_ || a=from then failwith "const capture substitution/origin preservation failed"
      | _ -> failwith "substitution erased array cast");
    let negatives=[src,dst,{operand with value=Values.VBottom},loan,dest;
      src,dst,{operand with ty=TNever},loan,dest;
      src,dst,{operand with value=Values.VBorrow(Values.VSharedBorrow(Values.BorrowId.of_int(-1),sid))},loan,dest;
      src,dst,{operand with value=Values.VBorrow(Values.VSharedBorrow(bid,Values.SharedBorrowId.of_int(-1)))},loan,dest;
      src,dst,operand,{loan with value=Values.VBottom},dest;
      src,dst,operand,{loan with ty=TNever},dest;
      src,dst,operand,{loan with value=Values.VSymbolic{sv with sv_id=Values.SymbolicValueId.of_int(-1)}},dest;
      src,dst,operand,{loan with value=Values.VSymbolic{sv with sv_ty=TNever}},dest;
      src,dst,operand,loan,TNever;
      src,TNever,operand,loan,dest] in
    List.iter(fun (a,b,v,l,d) ->
      let reached=ref false in
      let failed=try ignore(factory (fun t -> reached:=true;translate t) (fun _ -> l) a b v d);false with Errors.CFailure _ | Failure _ -> true in
      if not failed || !reached then failwith "malformed origin/loan/destination translated";
      incr rejected) negatives;
    if TypesUtils.ty_has_free_regions dest then (
      let visitor=object inherit [_] map_ty method! visit_RVar _ _ = RVar(Free(RegionId.of_int 100)) end in
      let wrong=visitor#visit_ty () dest in
      let reached=ref false in
      let failed=try ignore(factory (fun t -> reached:=true;translate t) (fun _ -> loan) src dst operand wrong);false with Errors.CFailure _ | Failure _ -> true in
      if not failed || !reached then failwith "wrong destination lifetime erased before checking";
      incr rejected);
    List.iter(fun backend -> Config.opt_backend:=Some backend;
      let reached=ref false in
      let failed=try ignore(factory (fun t -> reached:=true;translate t) (fun _ -> reached:=true;loan) src dst operand dest);false with Errors.CFailure _ | Failure _ -> true in
      if not failed || !reached then failwith "non-Lean conversion performed callbacks") [Config.Coq;Config.FStar;Config.HOL4];
    Config.opt_backend:=Some Config.Lean) !casts;
  Printf.printf "two actual Pure shared-array NonNull operators retain borrow/shared/loan identities and visible endpoints; distinct origins stay distinct and const substitution traverses source; %d origin/loan/destination controls and three backends rejected before type translation; Rust pointer extraction unproved\n" !rejected

let () =
  let c=match LlbcOfJson.crate_of_json_file Sys.argv.(1) with Ok c -> c | Error e -> failwith e in
  Config.fail_hard:=true;
  Config.warnings_as_errors:=true;
  Config.all_computable:=true;
  Config.split_files:=true;
  Config.use_lean_modules:=false;
  let prepared=PrePasses.apply_passes c in
  let _,pure=Translate.translate_crate_to_pure prepared ContextsBase.empty_marked_ids in
  let transparent=List.filter(fun (f : TranslateCore.fun_and_loops) -> f.f.body<>None) pure.fun_decls in
  if List.length transparent<>17 then failwith "full actual Pure body inventory changed";
  let args=List.filter(fun (f : TranslateCore.fun_and_loops) ->
    List.filter_map(function PeIdent(n,_) -> Some n | _ -> None) f.f.item_meta.name=["core";"fmt";"new"]) transparent in
  let f=match args with [f] -> f | _ -> failwith "actual Arguments.new Pure body missing" in
  let found=ref [] in
  let visitor=object
    inherit [_] Pure.iter_expr as super
    method! visit_cast_kind () op =
      (match op with Pure.CastRustSharedArrayNonNull(o,a,b) -> found:=(o,a,b)::!found | _ -> ());
      super#visit_cast_kind () op
  end in
  visitor#visit_texpr () (Option.get f.f.body).body;
  if List.length !found<>2 then failwith "actual postprocessed shared-array conversion inventory changed";
  List.iter(fun (o,a,b) ->
    if Values.BorrowId.to_int o.Pure.origin_borrow<0 || Values.SharedBorrowId.to_int o.Pure.origin_shared_borrow<0
       || Values.SymbolicValueId.to_int o.Pure.origin_loan_symbolic<0 || a=b then failwith "actual origin/endpoints malformed") !found;
  Printf.printf "full actual Pure translation/postprocessing retains17 bodies and both shared-array NonNull operators in Arguments.new with nonnegative actual borrow/shared/loan identities; no Lean extraction performed\n"
