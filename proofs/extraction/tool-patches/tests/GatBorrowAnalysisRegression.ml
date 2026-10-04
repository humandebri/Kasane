open Aeneas
open Types
open LlbcAst

let () =
  let crate = match LlbcOfJson.crate_of_json_file Sys.argv.(1) with
    | Ok c -> c | Error e -> failwith e in
  let tested = ref 0 in
  FunDeclId.Map.iter (fun _ f ->
    match f.src with
    | TraitDefaultFun (tref, method_id) when
        List.exists (function PeIdent ("slice_len", _) -> true | _ -> false) f.item_meta.name ->
        incr tested;
        let decl = TraitDeclId.Map.find tref.id crate.trait_decls in
        let method_ = (TraitMethodId.Map.find method_id decl.methods).binder_value in
        (match f.signature.output with
         | TRef (_, TSlice _, RShared) -> ()
         | _ -> failwith "expected actual borrowed-slice default signature");
        (match method_.signature.output with
         | TTraitType (_, _, args) when List.length args.regions = 1 -> ()
         | _ -> failwith "expected actual declared GAT return with one lifetime");
        (* Neither type has an ADT needing a declaration lookup. *)
        let infos = TypeDeclId.Map.empty in
        let concrete = TypesAnalysis.analyze_ty None infos f.signature.output in
        let abstract = TypesAnalysis.analyze_ty None infos method_.signature.output in
        if not concrete.contains_borrow then failwith "borrowed-slice default was not recognized";
        if abstract.contains_borrow then failwith "expected current unsupported-GAT analysis";
        Printf.printf "actual default signature contains_borrow=%b; corresponding declared GAT contains_borrow=%b\n"
          concrete.contains_borrow abstract.contains_borrow
    | _ -> ()) crate.fun_decls;
  if !tested <> 1 then failwith "expected exactly one actual slice_len default";
  print_endline "Type-analysis gap reproduced on actual extracted signatures; checker correctness remains unproved."
