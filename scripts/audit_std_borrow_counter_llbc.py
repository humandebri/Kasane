#!/usr/bin/env python3
"""Audit actual fixed std borrow-counter predicate bodies; no full RefCell proof."""
import copy,json,hashlib
from audit_jumpdest_llbc import ident


def inspect(data):
    if data["has_errors"]: raise ValueError("Extraction errors")
    x=data["translated"];cache={}
    def collect(v):
        if isinstance(v,dict):
            item=v.get("Value")
            if isinstance(item,list) and len(item)==2 and isinstance(item[1],dict) and list(item[1])==["Scalar"]:
                if item[0] in cache and cache[item[0]]!=item[1]:raise ValueError("Cache conflict")
                cache[item[0]]=item[1]
            for a in v.values(): collect(a)
        elif isinstance(v,list):
            for a in v: collect(a)
    collect(x)
    def resolve(v):
        if "Deduplicated" in v and v["Deduplicated"] not in cache: raise ValueError("Missing type cache entry")
        return cache[v["Deduplicated"]] if "Deduplicated" in v else v["Value"][1]
    fs=[f for f in x["fun_decls"] if f and isinstance(f["body"],dict)]
    if {ident(f) for f in fs}!={"core::cell::is_reading","core::cell::is_writing","core::cell::UNUSED"}:raise ValueError("Body inventory changed")
    funcs={ident(f):f for f in fs}; isize={"Scalar":{"Integer":{"Signed":"Isize"}}};boolean={"Scalar":"Bool"}
    def stmts(f):return f["body"]["Structured"]["body"]["statements"]
    def tags(f):return [next(iter(s["kind"])) if isinstance(s["kind"],dict) else s["kind"] for s in stmts(f)]
    g=next(g for g in x["global_decls"] if g and ident(g)=="core::cell::UNUSED")
    if g["global_kind"]!="NamedConst" or resolve(g["ty"])!=isize or g["value"]["Value"][1][0]["Call"][0]["kind"]!={"Fun":funcs["core::cell::UNUSED"]["def_id"]}:raise ValueError("UNUSED initializer link changed")
    for suffix,op in [("is_reading","Gt"),("is_writing","Lt")]:
        f=funcs["core::cell::"+suffix];sg=f["signature"]
        if sg["is_unsafe"] or sg["abi"]!="Rust" or sg["is_variadic"] or [resolve(t) for t in sg["inputs"]]!=[isize] or resolve(sg["output"])!=boolean:raise ValueError("Predicate signature changed")
        if any(f["generics"].values()):raise ValueError("Predicate generics changed")
        if tags(f)!=["StorageLive","StorageLive","Assign","Assign","StorageDead","StorageDead","Return"]:raise ValueError("Predicate operation inventory changed")
        assigns=[s["kind"]["Assign"] for s in stmts(f) if isinstance(s["kind"],dict) and "Assign" in s["kind"]]
        if assigns[0][0]["kind"]!={"Local":2} or assigns[0][1]["Use"][0]["Copy"]["kind"]!={"Local":1}:raise ValueError("Counter input transfer changed")
        lhs,rhs=assigns[1];binary=rhs["BinaryOp"]
        if lhs["kind"]!={"Local":0} or binary[0]!=op or binary[1]["Move"]["kind"]!={"Local":2} or binary[2]["Copy"]["kind"]["Global"]["id"]!=g["def_id"]:raise ValueError("Sign predicate changed")
    f=funcs["core::cell::UNUSED"];sg=f["signature"]
    if sg["inputs"] or resolve(sg["output"])!=isize or tags(f)!=["StorageLive","Assign","Return"]:raise ValueError("UNUSED signature/body changed")
    rhs=stmts(f)[1]["kind"]["Assign"][1]
    if rhs["Use"][0]["Const"]["Value"][1][0]!={"Integer":{"Signed":["Isize","0"]}}:raise ValueError("UNUSED value changed")
    def normalize(v):
        if isinstance(v,dict): return {k:normalize(a) for k,a in v.items() if k not in ["span","comments_before"] and not(k=="id" and "span" in v and "kind" in v)}
        if isinstance(v,list):return [normalize(a) for a in v]
        return v
    shape=hashlib.sha256(json.dumps([normalize(f["body"]) for f in fs],sort_keys=True).encode()).hexdigest()
    return dict(transparent_bodies=3,reading_operator="Gt",writing_operator="Lt",counter_type="Isize",unused=0,body_shape_sha256=shape,semantic_equivalence_proved=False)


def negative_controls(data):
    for label in ["reading_op","writing_op","input_origin","comparison_input","unused_value","wrong_global","bool_input","return"]:
        bad=copy.deepcopy(data);x=bad["translated"];f=next(f for f in x["fun_decls"] if f and ident(f)=="core::cell::is_reading");b=f["body"]["Structured"]["body"]["statements"]
        if label=="reading_op":b[3]["kind"]["Assign"][1]["BinaryOp"][0]="Ge"
        elif label=="writing_op":next(f for f in x["fun_decls"] if f and ident(f)=="core::cell::is_writing")["body"]["Structured"]["body"]["statements"][3]["kind"]["Assign"][1]["BinaryOp"][0]="Le"
        elif label=="input_origin":b[2]["kind"]["Assign"][1]["Use"][0]["Copy"]["kind"]={"Local":0}
        elif label=="comparison_input":b[3]["kind"]["Assign"][1]["BinaryOp"][1]["Move"]["kind"]={"Local":1}
        elif label=="unused_value":next(f for f in x["fun_decls"] if f and ident(f)=="core::cell::UNUSED")["body"]["Structured"]["body"]["statements"][1]["kind"]["Assign"][1]["Use"][0]["Const"]["Value"][1][0]["Integer"]["Signed"][1]="1"
        elif label=="wrong_global":b[3]["kind"]["Assign"][1]["BinaryOp"][2]["Copy"]["kind"]["Global"]["id"]=-1
        elif label=="bool_input":f["signature"]["inputs"]=[copy.deepcopy(f["signature"]["output"])]
        else:b[-1]["kind"]="UnwindResume"
        try:inspect(bad)
        except ValueError:pass
        else:raise AssertionError("Changed std counter witness accepted: "+label)
