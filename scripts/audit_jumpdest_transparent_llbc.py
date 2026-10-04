#!/usr/bin/env python3
"""Audit the fixed actual transparent JUMPDEST route; not a trait-erasure proof."""
import copy
import json
from pathlib import Path
from audit_jumpdest_llbc import audit, ident, negative_controls


def transparent_audit(data):
    observation = audit(data, False)
    options = data["translated"]["options"]
    if not options["remove_unused_clauses"] or options["opaque"] or options["exclude"] or options["monomorphize"]:
        raise ValueError("Unexpected extraction scope")
    cache = {}
    def collect(x):
        if isinstance(x, dict):
            v = x.get("Value")
            if isinstance(v, list) and len(v) == 2 and isinstance(v[1], dict) and len(v[1]) == 1 and next(iter(v[1])) in ["Adt", "Ref", "Scalar", "TypeVar"]:
                if v[0] in cache and cache[v[0]] != v[1]: raise ValueError("Type cache conflict")
                cache[v[0]] = v[1]
            for y in x.values(): collect(y)
        elif isinstance(x, list):
            for y in x: collect(y)
    collect(data["translated"])
    def resolve(x): return cache[x["Deduplicated"]] if "Deduplicated" in x else x["Value"][1]
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
    leaf = next(f for f in data["translated"]["fun_decls"] if f and ident(f).endswith("::jumpdest"))
    if leaf["generics"]["trait_clauses"] or context["generics"]["trait_clauses"] or interp["generics"]["trait_clauses"]: raise ValueError("Unused item clauses retained")
    if leaf["signature"]["is_unsafe"] or leaf["signature"]["abi"] != "Rust" or leaf["signature"]["is_variadic"]: raise ValueError("Leaf ABI changed")
    input_ty = resolve(leaf["signature"]["inputs"][0])["Adt"]
    args = input_ty["generics"]
    if input_ty["id"] != context["def_id"] or args["regions"] != [{"Var":{"Free":0}}] or args["trait_refs"] or args["const_generics"]:
        raise ValueError("Leaf Context/lifetime arguments changed")
    if [resolve(t) for t in args["types"]] != [{"TypeVar":{"Free":i}} for i in [1,0,2,3,4,5,6,7,8,9]]:
        raise ValueError("Leaf component arguments changed")
    if len(leaf["generics"]["regions"]) != 1 or len(leaf["generics"]["types_outlive"]) != 2:
        raise ValueError("Leaf lifetime/outlives inventory changed")
    for c, index in zip(leaf["generics"]["types_outlive"], [0,1]):
        if c["regions"] or resolve(c["skip_binder"][0]) != {"TypeVar":{"Free":index}} or c["skip_binder"][1] != {"Var":{"Free":0}}:
            raise ValueError("Leaf outlives relation changed")
    for d, count in [(leaf,10),(context,10),(interp,9),(gas,0),(memory,0)]:
        if len(d["generics"]["types"]) != count: raise ValueError("Type parameter inventory changed")
    observation.update(transparent_interpreter_fields=8, transparent_gas_fields=4,
                       transparent_memory_gas_fields=2, leaf_trait_clauses=0)
    return observation


def transparent_negative_controls(data):
    negative_controls(data, False)
    for label in ["opaque_interpreter","swapped_component","missing_gas","unsigned_refund"]:
        bad = copy.deepcopy(data)
        interp = next(x for x in bad["translated"]["type_decls"] if x and ident(x).endswith("::Interpreter"))
        gas = next(x for x in bad["translated"]["type_decls"] if x and ident(x) == "revm_interpreter::gas::Gas")
        if label == "opaque_interpreter": interp["kind"] = "Opaque"
        elif label == "swapped_component": interp["kind"]["Struct"][0]["ty"] = interp["kind"]["Struct"][2]["ty"]
        elif label == "missing_gas": interp["kind"]["Struct"].pop(1)
        else: gas["kind"]["Struct"][2]["ty"] = gas["kind"]["Struct"][0]["ty"]
        try: transparent_audit(bad)
        except ValueError: pass
        else: raise AssertionError("Changed transparent witness accepted: " + label)


if __name__ == "__main__":
    root = Path(__file__).resolve().parent.parent
    data = json.loads((root/".local/proof-tools/revm-control-transparent/jumpdest.llbc").read_text())
    print(json.dumps(transparent_audit(data)))
    transparent_negative_controls(data)
    print("Twelve copied-witness changes rejected; trait-erasure and Rust execution refinement unproved.")
