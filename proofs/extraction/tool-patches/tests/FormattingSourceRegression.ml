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
  List.iter (fun ty ->
    let before = show_ty ty in
    let rejected = try ignore (SymbolicToPureTypes.translate_sty None ty); false
      with Errors.CFailure _ | Failure _ -> true in
    if not rejected then failwith "unsupported function pointer silently accepted";
    if before<>show_ty ty then failwith "function pointer metadata altered") !pointers;
  Printf.printf "actual function pointer signatures retained (%d occurrences); unsupported Pure translation strictly rejected\n" (List.length !pointers);
  print_endline "exact three type/four function mappings removed; every other Lean mapping unchanged"
