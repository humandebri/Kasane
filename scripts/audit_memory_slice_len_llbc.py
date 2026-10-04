#!/usr/bin/env python3
"""Audit actual generic step call ordering and callback ABI; no execution theorem."""
import copy
import json
from pathlib import Path
from audit_jumpdest_llbc import ident


def inspect(data, transparent_ref):
    if data["has_errors"]: raise ValueError("Extraction errors")
    x=data["translated"]; cache={}
    def collect(value):
        if isinstance(value,dict):
            item=value.get("Value")
            if isinstance(item,list) and len(item)==2 and isinstance(item[1],dict) and len(item[1])==1 and next(iter(item[1])) in ["Adt","Ref","Scalar","TypeVar","Slice"]:
                if item[0] in cache and cache[item[0]] != item[1]: raise ValueError("Cache conflict")
                cache[item[0]]=item[1]
            for v in value.values(): collect(v)
        elif isinstance(value,list):
            for v in value: collect(v)
    collect(x)
    def resolve(ty): return cache[ty["Deduplicated"]] if "Deduplicated" in ty else ty["Value"][1]
    fs=[f for f in x["fun_decls"] if f and isinstance(f["body"],dict)]
    if len(fs)!=1 or ident(fs[0])!="revm_interpreter::interpreter_types::MemoryTr::slice_len": raise ValueError("Body inventory changed")
    f=fs[0]; sg=f["signature"]
    if sg["is_unsafe"] or sg["abi"]!="Rust" or sg["is_variadic"] or len(sg["inputs"])!=3: raise ValueError("Signature changed")
    ref=resolve(sg["inputs"][0])["Ref"]
    if ref[0]!={"Var":{"Free":0}} or ref[2]!="Shared" or resolve(ref[1])!={"TypeVar":{"Free":0}}: raise ValueError("Receiver lifetime/type changed")
    usize={"Scalar":{"Integer":{"Unsigned":"Usize"}}}
    if [resolve(t) for t in sg["inputs"][1:]] != [usize,usize]: raise ValueError("Range inputs changed")
    result=resolve(sg["output"])["Adt"]; decl=x["type_decls"][result["id"]]
    if ident(decl)!="core::cell::Ref" or result["generics"]["regions"]!=[{"Var":{"Free":0}}]: raise ValueError("Returned Ref lifetime changed")
    if transparent_ref:
        if not isinstance(decl["kind"],dict) or [field["name"] for field in decl["kind"]["Struct"]]!=["value","borrow"]: raise ValueError("Ref fields changed")
        value=resolve(decl["kind"]["Struct"][0]["ty"])["Adt"]; borrow=resolve(decl["kind"]["Struct"][1]["ty"])["Adt"]
        if ident(x["type_decls"][value["id"]])!="core::ptr::non_null::NonNull" or borrow["generics"]["regions"]!=[{"Var":{"Free":0}}] or ident(x["type_decls"][borrow["id"]])!="core::cell::BorrowRef": raise ValueError("Ref pointer/borrow token changed")
    elif decl["kind"]!="Opaque": raise ValueError("Opaque probe scope changed")
    stmts=f["body"]["Structured"]["body"]["statements"]
    assigns=[s["kind"]["Assign"] for s in stmts if isinstance(s["kind"],dict) and "Assign" in s["kind"]]
    binaries=[a[1]["BinaryOp"] for a in assigns if "BinaryOp" in a[1]]
    if len(binaries)!=1 or binaries[0][0]!={"Add":"Panic"} or binaries[0][1]["Copy"]["kind"]!={"Local":8} or binaries[0][2]["Copy"]["kind"]!={"Local":9}: raise ValueError("Checked offset addition changed")
    aggregate=next(a[1]["Aggregate"] for a in assigns if "Aggregate" in a[1])
    range_id=aggregate[0]["Adt"][0]["id"]
    if ident(x["type_decls"][range_id])!="core::ops::range::Range": raise ValueError("Range type changed")
    if [a["Move"]["kind"] for a in aggregate[1]] != [{"Local":6},{"Local":7}]: raise ValueError("Range start/end changed")
    calls=[s["kind"]["Call"]["call"] for s in stmts if isinstance(s["kind"],dict) and "Call" in s["kind"]]
    if len(calls)!=1 or [a["Move"]["kind"] for a in calls[0]["args"]] != [{"Local":4},{"Local":5}]: raise ValueError("Memory slice arguments changed")
    tr,method=calls[0]["func"]["Regular"]["kind"]["Trait"]
    trait_id=tr["Value"][1]["trait_decl_ref"]["skip_binder"]["id"]; trait=x["trait_decls"][trait_id]
    if ident(trait)!="revm_interpreter::interpreter_types::MemoryTr" or trait["methods"][method]["skip_binder"]["name"]!="slice": raise ValueError("Memory method changed")
    if stmts[-1]["kind"]!="Return": raise ValueError("Return changed")
    gat=next(t for t in x["trait_decls"] if t and ident(t).endswith("::MemoryTr"))["types"][0]
    if len(gat["params"]["regions"])!=1 or len(gat["params"]["types_outlive"])!=1 or len(gat["params"]["trait_type_constraints"])!=1 or gat["skip_binder"]["default"] is None or len(gat["skip_binder"]["implied_clauses"])!=1: raise ValueError("GAT metadata lost")
    import hashlib
    def canonical(value):
        if isinstance(value,dict): return {k:canonical(v) for k,v in value.items() if k not in ["span","comments_before"] and not (k=="id" and "kind" in value and "span" in value)}
        if isinstance(value,list): return [canonical(v) for v in value]
        return value
    digest=hashlib.sha256(json.dumps(canonical(f["body"]),sort_keys=True).encode()).hexdigest()
    return dict(transparent_functions=1,transparent_ref=transparent_ref,shared_receiver_region=0,returned_ref_region=0,checked_offset_add=True,slice_trait_call=True,body_shape_sha256=digest,semantic_equivalence_proved=False)


def negative_controls(data, transparent_ref):
    for label in ["add_wrap","range_reverse","wrong_arg","wrong_method","return","gat_binder"]:
        bad=copy.deepcopy(data);x=bad["translated"];f=next(f for f in x["fun_decls"] if f and isinstance(f["body"],dict));stmts=f["body"]["Structured"]["body"]["statements"]
        assigns=[s["kind"]["Assign"] for s in stmts if isinstance(s["kind"],dict) and "Assign" in s["kind"]]
        call=next(s["kind"]["Call"]["call"] for s in stmts if isinstance(s["kind"],dict) and "Call" in s["kind"])
        if label=="add_wrap": next(a[1]["BinaryOp"] for a in assigns if "BinaryOp" in a[1])[0]={"Add":"Wrap"}
        elif label=="range_reverse": next(a[1]["Aggregate"] for a in assigns if "Aggregate" in a[1])[1].reverse()
        elif label=="wrong_arg": call["args"][0]["Move"]["kind"]={"Local":5}
        elif label=="wrong_method": call["func"]["Regular"]["kind"]["Trait"][1]=0
        elif label=="return": stmts[-1]["kind"]="UnwindResume"
        else: next(t for t in x["trait_decls"] if t and ident(t).endswith("::MemoryTr"))["types"][0]["params"]["regions"]=[]
        try: inspect(bad,transparent_ref)
        except ValueError: pass
        else: raise AssertionError("Changed slice witness accepted: "+label)
