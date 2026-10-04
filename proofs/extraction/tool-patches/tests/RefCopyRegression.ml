open Aeneas
open LlbcAst
open Expressions
open Types

let copies f =
  let n = ref 0 in
  let v = object
    inherit [_] iter_statement as super
    method! visit_statement env st =
      (match st.kind with
       | Assign ({ ty = TRef (_, _, RMut); _ }, Use (Copy _, _)) -> incr n
       | _ -> ());
      super#visit_statement env st
  end in
  (match f.body with StructuredBody b -> v#visit_block () b.body | _ -> ());
  !n

let mutate mode f =
  let v = object
    inherit [_] map_statement as super
    method! visit_block env block =
      let block = super#visit_block env block in
      let rec go (statements : statement list) = match statements with
      | ({kind = Assign (({ty = TRef (_, _, RMut); _} as tmp), Use (Copy source, NoRetag)); _} as cp)
        :: ({kind = Assign (dst, RvRef (place, kind, metadata)); _} as br) :: rest ->
          let tail = go rest in
          (match mode with
          | "additional-use" -> cp :: br :: {br with kind = PlaceMention tmp} :: tail
          | "retag" -> {cp with kind = Assign (tmp, Use (Copy source, YesRetag))} :: br :: tail
          | "nonadjacent" -> cp :: {br with kind = Nop} :: br :: tail
          | "destination-alias" -> cp :: {br with kind = Assign (source, RvRef (place, kind, metadata))} :: tail
          | "unsupported-metadata" -> cp :: {br with kind = Assign (dst, RvRef (place, kind, Copy source))} :: tail
          | "self-copy" -> {cp with kind = Assign (tmp, Use (Copy tmp, NoRetag))} :: br :: tail
          | _ -> failwith "unknown mutation")
      | st :: rest -> st :: go rest
      | [] -> [] in
      {block with statements = go block.statements}
  end in
  match f.body with
  | StructuredBody b -> {f with body = StructuredBody {b with body = v#visit_block () b.body}}
  | _ -> f

let () =
  let crate = match LlbcOfJson.crate_of_json_file Sys.argv.(1) with
    | Ok c -> c | Error e -> failwith e in
  let tested = ref 0 in
  FunDeclId.Map.iter (fun _ f ->
    if copies f > 0 then begin
      incr tested;
      let out = PrePasses.fuse_immediate_ref_copy crate f in
      if copies out >= copies f then failwith "actual copy/reborrow pattern was not fused";
      Printf.printf "actual body: %d -> %d mutable reference copies\n" (copies f) (copies out);
      List.iter (fun mode ->
        let negative = mutate mode f in
        if negative = f then failwith ("mutation did not apply: " ^ mode);
        let result = PrePasses.fuse_immediate_ref_copy crate negative in
        if result <> negative then failwith ("unsafe fusion accepted: " ^ mode);
        Printf.printf "strictly preserved: %s\n" mode)
        ["additional-use"; "retag"; "nonadjacent"; "destination-alias"; "unsupported-metadata"; "self-copy"]
    end) crate.fun_decls;
  if !tested <> 2 then failwith "expected actual ADD and SUB bodies only";
  print_endline "Actual-pattern/guard regressions passed; semantic preservation is not proved."
