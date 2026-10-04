#!/usr/bin/env python3
"""Inspect actual fixed LLBC inputs; no Rust/LLBC semantic equivalence theorem."""
import copy
import json
from pathlib import Path


def ident(item):
    return "::".join(x["Ident"][0] for x in item["item_meta"]["name"] if "Ident" in x)


def audit(data, mono):
    if data["has_errors"]:
        raise ValueError("Extraction contains errors")
    types = {}
    def walk(x):
        if isinstance(x, dict):
            if x.get("has_errors") or "Error" in x:
                raise ValueError("Error node")
            v = x.get("Value")
            if isinstance(v, list) and len(v) == 2 and isinstance(v[1], dict):
                payload = v[1]
                if (isinstance(payload.get("Adt"), dict) and "builtin" in payload["Adt"]) or isinstance(payload.get("Ref"), list):
                    if v[0] in types and types[v[0]] != payload:
                        raise ValueError("Conflicting type cache entry")
                    types[v[0]] = payload
            for value in x.values(): walk(value)
        elif isinstance(x, list):
            for value in x: walk(value)
    walk(data["translated"])
    def resolve(x):
        return types[x["Deduplicated"]] if "Deduplicated" in x else x["Value"][1]
    def unit(x):
        a = resolve(x).get("Adt", {})
        return a.get("builtin") == "Tuple" and a["generics"] == dict(regions=[], types=[], const_generics=[], trait_refs=[])
    transparent = [f for f in data["translated"]["fun_decls"] if f and isinstance(f["body"], dict) and "Structured" in f["body"]]
    expected = {"revm_interpreter::instructions::control::jumpdest"}
    if mono: expected.add("kasane_revm_instruction_probe::opcode_jumpdest")
    if {ident(f) for f in transparent} != expected:
        raise ValueError("Actual function inventory changed")
    leaf = next(f for f in transparent if ident(f).endswith("::jumpdest"))
    body = leaf["body"]["Structured"]
    if body["locals"]["arg_count"] != 1 or not unit(leaf["signature"]["output"]):
        raise ValueError("Actual leaf signature changed")
    stmts = [s["kind"] for s in body["body"]["statements"]]
    if len(stmts) != 5 or stmts[0] != {"StorageLive": 0} or stmts[3] != {"StorageDead": 1} or stmts[4] != "Return":
        raise ValueError("Actual empty-body frame sequence changed")
    for stmt in stmts[1:3]:
        if set(stmt) != {"Assign"}: raise ValueError("Non-unit statement")
        lhs, rhs = stmt["Assign"]
        if lhs["kind"] != {"Local": 0} or not unit(lhs["ty"]):
            raise ValueError("Write outside local unit return")
        if set(rhs) != {"Aggregate"}: raise ValueError("Non-unit expression")
        aggregate, operands = rhs["Aggregate"]
        a, variant, field = aggregate["Adt"]
        if a["builtin"] != "Tuple" or operands or variant is not None or field is not None or a["generics"] != dict(regions=[], types=[], const_generics=[], trait_refs=[]):
            raise ValueError("Non-unit aggregate")
    contexts = [t for t in data["translated"]["type_decls"] if t and ident(t) == "revm_interpreter::instruction_context::InstructionContext"]
    if len(contexts) != 1: raise ValueError("Context inventory changed")
    ctx = contexts[0]
    fields = ctx["kind"]["Struct"]
    if [f["name"] for f in fields] != ["interpreter", "host"]:
        raise ValueError("Context fields changed")
    refs = [resolve(f["ty"])["Ref"] for f in fields]
    expected_region = "Erased" if mono else {"Var": {"Free": 0}}
    if any(ref[0] != expected_region or ref[2] != "Mut" for ref in refs):
        raise ValueError("Reference kind/region changed")
    if len(ctx["generics"]["regions"]) != (0 if mono else 1):
        raise ValueError("Context lifetime binder changed")
    traits = [t for t in data["translated"]["trait_decls"] if t]
    gat_metadata = None
    if mono:
        if traits: raise ValueError("Monomorphic trait inventory changed")
    else:
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
    return {"transparent_bodies": len(transparent), "actual_leaf_unit_only": True,
            "context_mutable_reference_fields": 2, "context_lifetime_binders": len(ctx["generics"]["regions"]),
            "context_reference_regions": [ref[0] for ref in refs], "traits": len(traits),
            "gat_metadata": gat_metadata, "semantic_equivalence_proved": False}


def negative_controls(data, mono):
    labels = ["write_argument", "nonempty_aggregate", "context_ref_kind"]
    if not mono: labels += ["gat_region", "gat_outlives", "gat_constraint", "gat_default", "gat_bound"]
    for label in labels:
        bad = copy.deepcopy(data)
        leaf = next(f for f in bad["translated"]["fun_decls"] if f and ident(f).endswith("::jumpdest"))
        if label == "write_argument":
            leaf["body"]["Structured"]["body"]["statements"][1]["kind"]["Assign"][0]["kind"] = {"Local": 1}
        elif label == "nonempty_aggregate":
            leaf["body"]["Structured"]["body"]["statements"][1]["kind"]["Assign"][1]["Aggregate"][1].append({"unexpected": "operand"})
        elif label == "context_ref_kind":
            ctx = next(t for t in bad["translated"]["type_decls"] if t and ident(t).endswith("::InstructionContext"))
            ctx["kind"]["Struct"][0]["ty"]["Value"][1]["Ref"][2] = "Shared"
        else:
            memory = next(t for t in bad["translated"]["trait_decls"] if t and ident(t).endswith("::MemoryTr"))
            gat = memory["types"][0]
            if label == "gat_region": gat["params"]["regions"] = []
            elif label == "gat_outlives": gat["params"]["types_outlive"] = []
            elif label == "gat_constraint": gat["params"]["trait_type_constraints"] = []
            elif label == "gat_default": gat["skip_binder"]["default"] = None
            else: gat["skip_binder"]["implied_clauses"] = []
        try: audit(bad, mono)
        except ValueError: pass
        else: raise AssertionError("Changed fixed witness accepted: " + label)


if __name__ == "__main__":
    root = Path(__file__).resolve().parent.parent
    for mono, filename in [(False, "jumpdest.llbc"), (True, "jumpdest-mono.llbc")]:
        data = json.loads((root / ".local/proof-tools/revm-control-leaf" / filename).read_text())
        print(filename, json.dumps(audit(data, mono)))
        negative_controls(data, mono)
    print("Eleven changed-body/reference/GAT metadata witnesses rejected. No instruction theorem claimed.")
