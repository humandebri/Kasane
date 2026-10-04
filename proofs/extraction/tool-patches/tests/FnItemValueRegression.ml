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
