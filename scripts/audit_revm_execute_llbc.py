#!/usr/bin/env python3
"""Audit actual execute source/typed callback transfer; trait erasure remains unproved."""
import copy
import json
from pathlib import Path
from audit_jumpdest_llbc import ident


def inspect(data):
    if data["has_errors"]: raise ValueError("Extraction errors")
    x = data["translated"]
    caches = {"type": {}, "trait": {}}
    def collect(v):
        if isinstance(v, dict):
            if "Error" in v: raise ValueError("Error node")
            item = v.get("Value")
            if isinstance(item, list) and len(item) == 2 and isinstance(item[1], dict):
                payload = item[1]
                ns = "trait" if "trait_decl_ref" in payload else ("type" if len(payload) == 1 and next(iter(payload)) in ["Adt","Ref","FnPtr","Scalar","TypeVar"] else None)
                if ns:
                    if item[0] in caches[ns] and caches[ns][item[0]] != payload: raise ValueError("Cache conflict")
                    caches[ns][item[0]] = payload
            for w in v.values(): collect(w)
        elif isinstance(v, list):
            for w in v: collect(w)
    collect(x)
    def resolve(v, ns="type"): return caches[ns][v["Deduplicated"]] if "Deduplicated" in v else v["Value"][1]
    def fn(suffix):
        matches = [f for f in x["fun_decls"] if f and ident(f) == suffix]
        if len(matches) != 1: raise ValueError("Function missing: " + suffix)
        return matches[0]
    transparent = [f for f in x["fun_decls"] if f and isinstance(f["body"],dict)]
    if {ident(f) for f in transparent} != {"revm_interpreter::instructions::execute"}:
        raise ValueError("Transparent inventory changed")
    def body(f): return f["body"]["Structured"]["body"]
    def calls(b): return [s["kind"]["Call"]["call"] for s in b["statements"] if isinstance(s["kind"],dict) and "Call" in s["kind"]]
    def target(c):
        f = c["func"]
        if "Dynamic" in f: return "dynamic"
        kind = f["Regular"]["kind"]
        if "Fun" in kind: return ident(x["fun_decls"][kind["Fun"]])
        ref, method_id = kind["Trait"]
        trait = x["trait_decls"][resolve(ref,"trait")["trait_decl_ref"]["skip_binder"]["id"]]
        return ident(trait) + "::" + trait["methods"][method_id]["skip_binder"]["name"]
    execute = fn("revm_interpreter::instructions::execute")
    statements=body(execute)["statements"]
    tags=[next(iter(stmt["kind"])) if isinstance(stmt["kind"],dict) else stmt["kind"] for stmt in statements]
    if tags != ["StorageLive","Assign","StorageLive","Assign","StorageLive","Assign","Call","StorageDead","StorageDead","StorageDead","StorageDead","Return"]:
        raise ValueError("Execute body operations changed")
    unwind=statements[6]["kind"]["Call"]["on_unwind"]["statements"]
    if [stmt["kind"] for stmt in unwind] != [{"StorageDead":2},{"StorageDead":1},"UnwindResume"]:
        raise ValueError("Execute unwind changed")
    if execute["signature"]["is_unsafe"] or execute["signature"]["abi"] != "Rust" or execute["signature"]["is_variadic"]:
        raise ValueError("Execute ABI changed")
    dynamic = calls(body(execute))
    if len(dynamic) != 1 or target(dynamic[0]) != "dynamic" or len(dynamic[0]["args"]) != 1: raise ValueError("Callback call changed")
    assignments = [stmt["kind"]["Assign"] for stmt in body(execute)["statements"] if isinstance(stmt["kind"],dict) and "Assign" in stmt["kind"]]
    if len(assignments) != 3: raise ValueError("Callback transfer inventory changed")
    lhs, rhs = assignments[1]
    origin = rhs["Use"][0]["Copy"]["kind"]
    if lhs["kind"] != {"Local":3} or origin != {"Projection":[{"kind":{"Local":1},"ty":origin["Projection"][0]["ty"]},{"Field":[None,0]}]}:
        raise ValueError("Callback is not copied from self.fn_")
    lhs, rhs = assignments[2]
    if lhs["kind"] != {"Local":4} or rhs["Use"][0]["Move"]["kind"] != {"Local":2}:
        raise ValueError("Original Context argument transfer changed")
    if dynamic[0]["func"]["Dynamic"]["Move"]["kind"] != {"Local":3} or dynamic[0]["args"][0]["Move"]["kind"] != {"Local":4} or dynamic[0]["dest"]["kind"] != {"Local":0}:
        raise ValueError("Actual callback/Context/result binding changed")
    instruction = next(t for t in x["type_decls"] if t and ident(t) == "revm_interpreter::instructions::Instruction")
    fields = instruction["kind"]["Struct"]
    if [f["name"] for f in fields] != ["fn_","static_gas"]: raise ValueError("Instruction fields changed")
    pointer = resolve(fields[0]["ty"])["FnPtr"]
    sig = pointer["skip_binder"]
    if len(pointer["regions"]) != 1 or sig["is_unsafe"] or sig["abi"] != "Rust" or sig["is_variadic"] or len(sig["inputs"]) != 1: raise ValueError("Callback ABI/binder changed")
    ctx = resolve(sig["inputs"][0])["Adt"]
    if ident(x["type_decls"][ctx["id"]]) != "revm_interpreter::instruction_context::InstructionContext" or ctx["generics"]["regions"] != [{"Var":{"Bound":[0,0]}}]: raise ValueError("Callback context lifetime changed")
    if [resolve(ty) for ty in ctx["generics"]["types"]] != [{"TypeVar":{"Free":i}} for i in [1,0,2,3,4,5,6,7,8,9]] or ctx["generics"]["trait_refs"] or ctx["generics"]["const_generics"]:
        raise ValueError("Callback component type binding changed")
    if resolve(assignments[1][0]["ty"]) != resolve(dynamic[0]["func"]["Dynamic"]["Move"]["ty"]):
        raise ValueError("Evaluated callback type differs from stored callback")
    self_args = resolve(execute["signature"]["inputs"][0])["Adt"]["generics"]["types"]
    substitutions = {i: resolve(arg) for i,arg in enumerate(self_args)}
    def expand(value, substitute=False):
        if isinstance(value,dict):
            if "Deduplicated" in value: return expand(resolve(value),substitute)
            if "Value" in value and isinstance(value["Value"],list) and len(value["Value"])==2 and isinstance(value["Value"][1],dict):
                return expand(value["Value"][1],substitute)
            if substitute and list(value)==["TypeVar"] and "Free" in value["TypeVar"]:
                return expand(substitutions[value["TypeVar"]["Free"]],False)
            return {k:expand(v,substitute) for k,v in value.items()}
        if isinstance(value,list): return [expand(v,substitute) for v in value]
        return value
    if expand(fields[0]["ty"],True) != expand(assignments[1][0]["ty"]):
        raise ValueError("Callback instance does not follow original self generic bindings")
    context_decl = x["type_decls"][ctx["id"]]
    if len(context_decl["generics"]["regions"]) != 1 or [f["name"] for f in context_decl["kind"]["Struct"]] != ["interpreter","host"]:
        raise ValueError("Callback Context shape changed")
    for field in context_decl["kind"]["Struct"]:
        ref = resolve(field["ty"])["Ref"]
        if ref[0] != {"Var":{"Free":0}} or ref[2] != "Mut": raise ValueError("Callback Context loan region/kind changed")
    unit = resolve(sig["output"])["Adt"]
    if unit["builtin"] != "Tuple" or unit["generics"] != dict(regions=[],types=[],const_generics=[],trait_refs=[]): raise ValueError("Callback result changed")
    decls = data["translated"]["type_decls"]
    def decl(name):
        matches = [x for x in decls if x and ident(x) == name]
        if len(matches) != 1 or "Struct" not in matches[0]["kind"]: raise ValueError("Transparent type missing: " + name)
        return matches[0]
    interp = decl("revm_interpreter::interpreter::Interpreter")
    gas = decl("revm_interpreter::gas::Gas")
    memory = decl("revm_interpreter::gas::MemoryGas")
    context = decl("revm_interpreter::instruction_context::InstructionContext")
    def fields(d): return d["kind"]["Struct"]
    def names(d): return [f["name"] for f in fields(d)]
    if names(interp) != ["bytecode", "gas", "stack", "return_data", "memory", "input", "runtime_flag", "extend"]: raise ValueError("Interpreter fields changed")
    for pos, index in [(0,3),(2,1),(3,4),(4,2),(5,5),(6,6),(7,7)]:
        if resolve(fields(interp)[pos]["ty"]) != {"TypeVar":{"Free":index}}: raise ValueError("Interpreter component type changed")
    def plain_adt(ty, target):
        return resolve(ty) == {"Adt":{"id":target["def_id"],"generics":dict(regions=[],types=[],const_generics=[],trait_refs=[]),"builtin":None}}
    if not plain_adt(fields(interp)[1]["ty"], gas): raise ValueError("Gas component changed")
    if names(gas) != ["limit","remaining","refunded","memory"] or names(memory) != ["words_num","expansion_cost"]: raise ValueError("Gas fields changed")
    u64 = {"Scalar":{"Integer":{"Unsigned":"U64"}}}
    i64 = {"Scalar":{"Integer":{"Signed":"I64"}}}
    usize = {"Scalar":{"Integer":{"Unsigned":"Usize"}}}
    if [resolve(fields(gas)[i]["ty"]) for i in range(3)] != [u64,u64,i64] or not plain_adt(fields(gas)[3]["ty"], memory): raise ValueError("Gas field types changed")
    if [resolve(f["ty"]) for f in fields(memory)] != [usize,u64]: raise ValueError("MemoryGas field types changed")
    if execute["generics"]["trait_clauses"] or context["generics"]["trait_clauses"] or interp["generics"]["trait_clauses"]: raise ValueError("Unused clauses retained")
    if len(execute["generics"]["regions"]) != 1 or len(execute["generics"]["types_outlive"]) != 2: raise ValueError("Execute lifetime constraints changed")
    options = x["options"]
    if not options["remove_unused_clauses"] or options["opaque"] or options["exclude"] or options["monomorphize"]: raise ValueError("Scope changed")
    traits = [t for t in x["trait_decls"] if t]
    memory = next(t for t in traits if ident(t) == "revm_interpreter::interpreter_types::MemoryTr")
    if len(memory["types"]) != 1:
        raise ValueError("GAT inventory changed")
    gat = memory["types"][0]
    counts = {k: len(v) for k, v in gat["params"].items()}
    if counts != dict(regions=1, types=0, const_generics=0, trait_clauses=0,
                      regions_outlive=0, types_outlive=1, trait_type_constraints=1):
        raise ValueError("GAT binder/constraint inventory changed")
    value = gat["skip_binder"]
    if value["name"] != "slice_len_ty" or value["default"] is None or len(value["implied_clauses"]) != 1:
        raise ValueError("GAT default/implied bound lost")
    bound_id = value["implied_clauses"][0]["trait_"]["skip_binder"]["id"]
    if ident(data["translated"]["trait_decls"][bound_id]) != "core::ops::deref::Deref":
        raise ValueError("GAT implied Deref bound changed")
    gat_metadata = dict(name=value["name"], binder_counts=counts,
                        default_present=True, implied_deref_clauses=1)
    return dict(transparent_functions=1, stored_callback_origin="self.fn_", context_origin="ctx", callback_binders=1, context_mut_refs=2, interpreter_fields=8, gas_fields=4, memory_gas_fields=2, gat_metadata=gat_metadata, semantic_equivalence_proved=False)

def negative_controls(data):
    for label in ["wrong_field","wrong_argument","wrong_callee","wrong_instance","unsafe","erased","gas_field","gat_binder","gat_default","body_return","unwind"]:
        bad=copy.deepcopy(data); x=bad["translated"]
        f=next(f for f in x["fun_decls"] if f and ident(f).endswith("::execute"))
        stmts=f["body"]["Structured"]["body"]["statements"]
        assigns=[s["kind"]["Assign"] for s in stmts if isinstance(s["kind"],dict) and "Assign" in s["kind"]]
        call=next(s["kind"]["Call"]["call"] for s in stmts if isinstance(s["kind"],dict) and "Call" in s["kind"])
        ins=next(t for t in x["type_decls"] if t and ident(t).endswith("::Instruction"))
        p=ins["kind"]["Struct"][0]["ty"]["Value"][1]["FnPtr"]
        if label=="wrong_field": assigns[1][1]["Use"][0]["Copy"]["kind"]["Projection"][1]["Field"][1]=1
        elif label=="wrong_argument": call["args"][0]["Move"]["kind"]={"Local":1}
        elif label=="wrong_callee": call["func"]["Dynamic"]["Move"]["kind"]={"Local":4}
        elif label=="wrong_instance":
            args=f["signature"]["inputs"][0]["Value"][1]["Adt"]["generics"]["types"]
            args[0],args[1]=args[1],args[0]
        elif label=="unsafe": p["skip_binder"]["is_unsafe"]=True
        elif label=="erased": p["skip_binder"]["inputs"][0]["Value"][1]["Adt"]["generics"]["regions"]=["Erased"]
        elif label=="gas_field": next(t for t in x["type_decls"] if t and ident(t).endswith("::Gas"))["kind"]["Struct"][0]["name"]="wrong"
        elif label=="body_return": stmts[-1]["kind"]="UnwindResume"
        elif label=="unwind": stmts[6]["kind"]["Call"]["on_unwind"]["statements"][-1]["kind"]="Return"
        else:
            g=next(t for t in x["trait_decls"] if t and ident(t).endswith("::MemoryTr"))["types"][0]
            if label=="gat_binder": g["params"]["regions"]=[]
            else: g["skip_binder"]["default"]=None
        try: inspect(bad)
        except ValueError: pass
        else: raise AssertionError("Changed execute witness accepted: "+label)
