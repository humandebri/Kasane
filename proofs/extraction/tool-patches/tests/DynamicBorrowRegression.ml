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
  print_endline "Actual callback generic/lifetime signature preserved; six unsupported signature controls rejected."
