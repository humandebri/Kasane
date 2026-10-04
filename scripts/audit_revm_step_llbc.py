#!/usr/bin/env python3
"""Audit actual generic step call ordering and callback ABI; no execution theorem."""
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
    expected = {"revm_interpreter::interpreter::step","revm_interpreter::instructions::static_gas",
                "revm_interpreter::gas::record_cost_unsafe","revm_interpreter::interpreter::halt_oog",
                "revm_interpreter::instructions::execute","revm_interpreter::interpreter::halt"}
    if {ident(f) for f in transparent} != expected: raise ValueError("Transparent inventory changed")
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
    step = fn("revm_interpreter::interpreter::step")
    cs = calls(body(step))
    expected_calls = ["revm_interpreter::interpreter_types::Jumps::opcode",
                      "revm_interpreter::interpreter_types::Jumps::relative_jump",
                      "core::array::as_slice","core::slice::get_unchecked",
                      "revm_interpreter::instructions::static_gas",
                      "revm_interpreter::gas::record_cost_unsafe",
                      "revm_interpreter::interpreter::halt_oog"]
    if [target(c) for c in cs] != expected_calls: raise ValueError("Step call order changed")
    if cs[1]["args"][1]["Const"]["Value"][1][0] != {"Integer":{"Signed":["Isize","1"]}}:
        raise ValueError("PC increment changed")
    switches = [s["kind"]["Switch"] for s in body(step)["statements"] if isinstance(s["kind"],dict) and "Switch" in s["kind"]]
    if len(switches) != 1: raise ValueError("OOG switch inventory changed")
    sw = switches[0]
    cond = sw["data"]
    if cond["scrutinee"]["Value"]["Move"]["kind"] != cs[5]["dest"]["kind"] or cond["fallback"] != 0 or len(cond["branches"]) != 1:
        raise ValueError("OOG result selection changed")
    case, branch = cond["branches"][0]
    if case["Value"][1][0] != {"Bool":False} or branch != 1: raise ValueError("OOG branch polarity changed")
    if calls(sw["branches"][0]) or [target(c) for c in calls(sw["branches"][1])] != ["revm_interpreter::instructions::execute"]:
        raise ValueError("Instruction executes on wrong branch")
    if sw["branches"][1]["statements"][-1]["kind"] != "Return": raise ValueError("Success branch does not return")
    if [target(c) for c in calls(body(fn("revm_interpreter::interpreter::halt_oog")))] != ["revm_interpreter::gas::spend_all","revm_interpreter::interpreter::halt"]:
        raise ValueError("OOG reset/halt order changed")
    execute = fn("revm_interpreter::instructions::execute")
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
    if resolve(assignments[1][0]["ty"]) != resolve(fields[0]["ty"]) or resolve(dynamic[0]["func"]["Dynamic"]["Move"]["ty"]) != resolve(fields[0]["ty"]):
        raise ValueError("Evaluated callback type differs from stored callback")
    context_decl = x["type_decls"][ctx["id"]]
    if len(context_decl["generics"]["regions"]) != 1 or [f["name"] for f in context_decl["kind"]["Struct"]] != ["interpreter","host"]:
        raise ValueError("Callback Context shape changed")
    for field in context_decl["kind"]["Struct"]:
        ref = resolve(field["ty"])["Ref"]
        if ref[0] != {"Var":{"Free":0}} or ref[2] != "Mut": raise ValueError("Callback Context loan region/kind changed")
    unit = resolve(sig["output"])["Adt"]
    if unit["builtin"] != "Tuple" or unit["generics"] != dict(regions=[],types=[],const_generics=[],trait_refs=[]): raise ValueError("Callback result changed")
    return dict(transparent_functions=6, step_top_level_call_order=expected_calls,
                pc_increment=1, execute_on_oog_false=True, callback_rust_safe=True,
                callback_lifetime_binders=1, callback_context_bound_region=[0,0],
                indirect_call_preserved=True, stored_callback_origin="self.fn_", context_origin="ctx",
                mutable_context_fields=2, semantic_equivalence_proved=False)


def negative_controls(data):
    for label in ["reorder","pc_increment","oog_polarity","direct_callback","unsafe_callback","erased_callback_lifetime","wrong_field","wrong_argument","wrong_callee"]:
        bad = copy.deepcopy(data);x=bad["translated"]
        step=next(f for f in x["fun_decls"] if f and ident(f)=="revm_interpreter::interpreter::step")
        stmts=step["body"]["Structured"]["body"]["statements"]
        calls=[s for s in stmts if isinstance(s["kind"],dict) and "Call" in s["kind"]]
        ins=next(t for t in x["type_decls"] if t and ident(t)=="revm_interpreter::instructions::Instruction")
        pointer=ins["kind"]["Struct"][0]["ty"]["Value"][1]["FnPtr"]
        if label=="reorder": calls[0]["kind"],calls[1]["kind"]=calls[1]["kind"],calls[0]["kind"]
        elif label=="pc_increment": calls[1]["kind"]["Call"]["call"]["args"][1]["Const"]["Value"][1][0]["Integer"]["Signed"][1]="2"
        elif label=="oog_polarity":
            sw=next(s["kind"]["Switch"] for s in stmts if isinstance(s["kind"],dict) and "Switch" in s["kind"])
            sw["data"]["branches"][0][0]["Value"][1][0]["Bool"]=True
        elif label=="direct_callback":
            ex=next(f for f in x["fun_decls"] if f and ident(f)=="revm_interpreter::instructions::execute")
            c=next(s["kind"]["Call"]["call"] for s in ex["body"]["Structured"]["body"]["statements"] if isinstance(s["kind"],dict) and "Call" in s["kind"])
            c["func"]=copy.deepcopy(calls[4]["kind"]["Call"]["call"]["func"])
        elif label=="unsafe_callback": pointer["skip_binder"]["is_unsafe"]=True
        elif label=="erased_callback_lifetime": pointer["skip_binder"]["inputs"][0]["Value"][1]["Adt"]["generics"]["regions"]=["Erased"]
        else:
            ex=next(f for f in x["fun_decls"] if f and ident(f)=="revm_interpreter::instructions::execute")
            body=ex["body"]["Structured"]["body"]["statements"]
            if label=="wrong_field":
                assigns=[s["kind"]["Assign"] for s in body if isinstance(s["kind"],dict) and "Assign" in s["kind"]]
                assigns[1][1]["Use"][0]["Copy"]["kind"]["Projection"][1]["Field"][1]=1
            else:
                c=next(s["kind"]["Call"]["call"] for s in body if isinstance(s["kind"],dict) and "Call" in s["kind"])
                if label=="wrong_argument": c["args"][0]["Move"]["kind"]={"Local":1}
                else: c["func"]["Dynamic"]["Move"]["kind"]={"Local":4}
        try: inspect(bad)
        except ValueError: pass
        else: raise AssertionError("Changed dispatch witness accepted: "+label)


if __name__=="__main__":
    root=Path(__file__).resolve().parent.parent
    data=json.loads((root/".local/proof-tools/revm-step-dispatch/step.llbc").read_text())
    print(json.dumps(inspect(data)));negative_controls(data)
    print("Nine call-order/PC/OOG/callback-origin witness changes rejected; no execution theorem.")
