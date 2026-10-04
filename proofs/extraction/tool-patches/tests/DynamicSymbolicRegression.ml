open Aeneas
open Types
let () =
  let crate = match LlbcOfJson.crate_of_json_file Sys.argv.(1) with
    | Ok crate -> crate | Error error -> failwith error in
  let instruction = TypeDeclId.Map.bindings crate.type_decls |> List.find_map (fun (_,decl) ->
    match decl.kind with
    | Struct ({ field_name = "fn_"; field_ty; _ } :: _) -> Some (decl,field_ty)
    | _ -> None) |> Option.get in
  let decl,ty = instruction in
  let pointer = match ty with TFnPtr pointer -> pointer | _ -> failwith "Missing pointer" in
  let signature = InterpStatements.dynamic_borrow_signature decl.item_meta.span decl.generics.types [] ty in
  if signature.item_binder_params.regions <> pointer.binder_regions
     || signature.item_binder_params.types <> decl.generics.types then failwith "Lost generic environment";
  (match signature.item_binder_value.inputs with
   | [ TAdt { generics = { regions = [ RVar (Free id) ]; types; _ }; _ } ] when id = RegionId.of_int 0 ->
       (match pointer.binder_value.inputs with
        | [ TAdt { generics = original; _ } ] when types = original.types -> ()
        | _ -> failwith "Component type arguments changed")
   | _ -> failwith "Callback region not lifted to signature parameter");
  let rejects = ref 0 in
  let reject label pointer =
    let failed = try
      ignore (InterpStatements.dynamic_borrow_signature decl.item_meta.span decl.generics.types [] (TFnPtr pointer)); false
      with Errors.CFailure _ | Failure _ -> true in
    if not failed then failwith ("Accepted unsupported signature: " ^ label);
    incr rejects in
  let sg = pointer.binder_value in
  reject "unsafe" { pointer with binder_value = { sg with is_unsafe = true } };
  reject "variadic" { pointer with binder_value = { sg with is_variadic = true } };
  reject "no binder" { pointer with binder_regions = [] };
  reject "non-unit" { pointer with binder_value = { sg with output = TScalar (TInteger (Unsigned U8)) } };
  let change_region region = match sg.inputs with
    | [ TAdt adt ] -> { pointer with binder_value = { sg with inputs = [ TAdt { adt with generics = { adt.generics with regions = [region] } } ] } }
    | _ -> failwith "Missing Context" in
  reject "captured region" (change_region (RVar (Free (RegionId.of_int 0))));
  reject "erased region" (change_region RErased);
  if !rejects <> 6 then failwith "Wrong rejection count";
  print_endline "Actual callback generic/lifetime signature preserved; six unsupported signature controls rejected.";

  let decls = Interp.compute_contexts crate in
  let execute = FunDeclId.Map.bindings crate.fun_decls |> List.find_map (fun (_,(f : LlbcAst.fun_decl)) ->
    match List.rev f.item_meta.name with
    | Types.PeIdent ("execute", _) :: _ -> Some f
    | _ -> None) |> Option.get in
  let ctx = InterpUtils.initialize_eval_ctx (Some execute.item_meta.span) decls []
      execute.generics.types execute.generics.const_generics Contexts.empty_marked_ids in
  let signature = InterpStatements.dynamic_borrow_signature decl.item_meta.span ctx.type_vars ctx.const_generic_vars ty in
  let args = TypesUtils.generic_args_of_params (Some decl.item_meta.span) signature.item_binder_params in
  let inst = InterpUtils.instantiate_fun_sig (Some decl.item_meta.span) ctx args Self signature in
  let effects = function
    | None -> { Pure.can_fail=true; can_diverge=true; is_rec=false }
    | Some _ -> { Pure.can_fail=false; can_diverge=true; is_rec=false } in
  let decomposed = SymbolicToPureTypes.translate_inst_fun_sig_with_effects
      (Some decl.item_meta.span) decls effects (fun _ -> "actual dynamic callback")
      inst signature.item_binder_value.output [Some "context"] in
  if not (Types.RegionGroupId.Map.for_all (fun _ (back : Pure.back_sg_info) ->
      back.effect_info.can_diverge && not back.effect_info.can_fail) decomposed.back_sg)
    then failwith "Backward divergence effect lost";
  let forward = SymbolicToPureTypes.translate_fwd_ty (Some decl.item_meta.span) decls (List.hd inst.inputs) in
  if decomposed.fwd_inputs <> [forward] then failwith "Forward context changed";
  if not decomposed.fwd_info.effect_info.can_fail || not decomposed.fwd_info.effect_info.can_diverge then failwith "Unknown callback effects lost";
  let backs = SymbolicToPureTypes.compute_back_tys decomposed in
  if backs <> [Some forward] then failwith "Original Context backward output lost";
  let output = SymbolicToPureTypes.compute_output_ty_from_decomposed decomposed in
  if output <> PureUtils.mk_result_ty forward then failwith "Callback output omitted Context or Result";
  print_endline "Actual dynamic callback decomposed without a declaration ID; forward Context and Result Context backward state retained."
;
  let regulars = ref 0 in
  FunDeclId.Map.iter (fun id (f : LlbcAst.fun_decl) ->
    match f.body with
    | _ when not (LlbcAstUtils.has_body f.body) -> ()
    | _ ->
      let span = Some f.item_meta.span in
      let ctx = InterpUtils.initialize_eval_ctx span decls [] f.generics.types
          f.generics.const_generics Contexts.empty_marked_ids in
      let _,inst = Interp.symbolic_instantiate_fun_sig f.item_meta.span ctx
          (LlbcAstUtils.bound_fun_sig_of_decl f) f.src in
      let names = List.map (fun _ -> None) inst.inputs in
      let fid = Pure.FunId id in
      let old = SymbolicToPureTypes.translate_inst_fun_sig_to_decomposed_fun_type
          span decls fid inst f.signature.output names in
      let current = SymbolicToPureTypes.translate_inst_fun_sig_with_effects span decls
          (fun gid -> SymbolicToPureTypes.compute_raw_fun_effect_info span decls.fun_ctx.fun_infos fid gid)
          (fun pctx -> PrintPure.regular_fun_id_to_string pctx (Pure.FromLlbc (fid,None)))
          inst f.signature.output names in
      if old <> current then failwith "Regular signature decomposition changed";
      incr regulars) crate.fun_decls;
  if !regulars <> 6 then failwith "Wrong regular signature count";
  print_endline "Six actual regular signatures retain identical decompositions through the original API."
;
  let crate = PrePasses.apply_passes crate in
  let decls = Interp.compute_contexts crate in
  let execute = FunDeclId.Map.bindings crate.fun_decls |> List.find_map (fun (_, (f : LlbcAst.fun_decl)) ->
    match List.rev f.item_meta.name with
    | Types.PeIdent ("execute", _) :: _ -> Some f
    | _ -> None) |> Option.get in
  let _,ast = Interp.evaluate_function_symbolic true decls Contexts.empty_marked_ids execute in
  let calls = ref [] in
  let visitor = object
    inherit [_] SymbolicAst.iter_expr
    method! visit_call _ (call : SymbolicAst.call) =
      match call.call_id with
      | SymbolicAst.Dynamic _ -> calls := call :: !calls
      | _ -> if call.callback <> None then failwith "Regular call carries callback"
  end in
  visitor#visit_expr () (Option.get ast);
  let call = match !calls with [call] -> call | _ -> failwith "Actual execute must retain one dynamic call" in
  let callback = Option.get call.callback in
  (match callback.ty with TFnPtr _ -> () | _ -> failwith "Lost evaluated callback type");
  (match callback.value with Values.VSymbolic _ -> () | _ -> failwith "Actual callback replaced by constant");
  if call.abstractions = [] || call.sg = None || call.inst_sg = None || List.length call.args <> 1 then
    failwith "Actual call lost context/signature/borrow abstractions";
  let pointer = match callback.ty with TFnPtr p -> p | _ -> assert false in
  let argument = List.hd call.args in
  if TypesUtils.ty_erase_regions argument.ty <>
     TypesUtils.ty_erase_regions (List.hd pointer.binder_value.inputs) then
    failwith "Dynamic argument is not original callback Context type";
  let id = match call.call_id with SymbolicAst.Dynamic id -> id | _ -> assert false in
  List.iter (fun abs_id ->
    let abs = Contexts.ctx_lookup_abs call.ctx abs_id in
    (match abs.kind with
     | Values.FunCall (actual,_) when actual = id -> ()
     | _ -> failwith "Dynamic borrow abstraction linked to wrong call");
    match abs.cont with
    | Some { output = Some _; input = Some _ } -> ()
    | _ -> failwith "Dynamic borrow continuation lost input or output") call.abstractions;
  let env = Print.Contexts.eval_ctx_to_fmt_env call.ctx in
  let text = PrintSymbolicAst.call_to_string env "" call in
  if String.length text = 0 then failwith "Dynamic call printing failed";
  print_endline "Actual execute symbolic AST retains evaluated callback, original Context argument, instantiated signature and borrow abstractions."
