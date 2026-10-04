#!/usr/bin/env python3
"""Audit lifetime-only LLBC fixtures; no monomorphizer/ref/provenance proof."""
import copy,json
from pathlib import Path


def name(item):
    return "::".join(x["Ident"][0] for x in item["item_meta"]["name"] if "Ident" in x)


def audit(data, mono):
    if data["has_errors"]: raise ValueError("Extraction error")
    cache={}
    def collect(x):
        if isinstance(x,dict):
            if x.get("has_errors") or "Error" in x: raise ValueError("Error node")
            v=x.get("Value")
            if isinstance(v,list) and len(v)==2 and isinstance(v[1],dict):
                p=v[1]
                if isinstance(p.get("Ref"),list) or isinstance(p.get("Scalar"),dict) or isinstance(p.get("Adt"),dict):
                    if v[0] in cache and cache[v[0]]!=p: raise ValueError("Type cache conflict")
                    cache[v[0]]=p
            for value in x.values():collect(value)
        elif isinstance(x,list):
            for value in x:collect(value)
    collect(data["translated"])
    def resolve(ty):return cache[ty["Deduplicated"]] if "Deduplicated" in ty else ty["Value"][1]
    regions={}
    for suffix,arity,field_ids in [("Same",1,[0,0]),("Split",2,[0,1])]:
        structs=[t for t in data["translated"]["type_decls"] if t and name(t)=="kasane_mono_lifetime_probe::"+suffix]
        if len(structs)!=1: raise ValueError("Type inventory")
        decl=structs[0]
        if len(decl["generics"]["regions"])!=(0 if mono else arity):raise ValueError("Lifetime binder changed")
        fields=decl["kind"]["Struct"]
        if [f["name"] for f in fields]!=["left","right"]:raise ValueError("Fields changed")
        refs=[resolve(f["ty"])["Ref"] for f in fields]
        for ref,index in zip(refs,field_ids):
            if ref[0]!=("Erased" if mono else {"Var":{"Free":index}}) or ref[2]!="Mut" or resolve(ref[1])!={"Scalar":{"Integer":{"Unsigned":"U32"}}}:
                raise ValueError("Field lifetime/kind/pointee changed")
        regions[suffix]=[ref[0] for ref in refs]
    fns=[f for f in data["translated"]["fun_decls"] if f and isinstance(f["body"],dict)]
    if {name(f).split("::")[-1] for f in fns}!={"same_noop","split_noop","split_left","split_right"}:raise ValueError("Function inventory")
    if not mono:
        for f in fns:
            n=name(f).split("::")[-1]
            arg=resolve(f["signature"]["inputs"][0])["Adt"]["generics"]["regions"]
            expected=[{"Var":{"Free":i}} for i in range(1 if n=="same_noop" else 2)]
            if arg!=expected:raise ValueError("Input lifetime arguments changed")
            if n in ["split_left","split_right"]:
                ref=resolve(f["signature"]["output"])["Ref"]
                if ref[0]!={"Var":{"Free":0 if n=="split_left" else 1}} or ref[2]!="Mut":raise ValueError("Returned reference origin changed")
    return {"transparent_functions":4,"same_field_regions":regions["Same"],"split_field_regions":regions["Split"],"declaration_binders": [0,0] if mono else [1,2],"semantic_equivalence_proved":False}


def negative_controls(data):
    for label in ["merge_split_regions","drop_split_binder","wrong_left_return","invent_same_region"]:
        bad=copy.deepcopy(data)
        same=next(t for t in bad["translated"]["type_decls"] if t and name(t).endswith("::Same"))
        split=next(t for t in bad["translated"]["type_decls"] if t and name(t).endswith("::Split"))
        if label=="merge_split_regions":split["kind"]["Struct"][1]["ty"]=same["kind"]["Struct"][1]["ty"]
        elif label=="drop_split_binder":split["generics"]["regions"].pop()
        elif label=="invent_same_region":same["kind"]["Struct"][1]["ty"]=split["kind"]["Struct"][1]["ty"]
        else:
            left=next(f for f in bad["translated"]["fun_decls"] if f and name(f).endswith("::split_left"))
            right=next(f for f in bad["translated"]["fun_decls"] if f and name(f).endswith("::split_right"))
            left["signature"]["output"]=right["signature"]["output"]
        try:audit(bad,False)
        except ValueError:pass
        else:raise AssertionError("Changed lifetime relationship accepted: "+label)


if __name__=="__main__":
    base=Path(__file__).resolve().parent.parent/'.local/proof-tools/mono-lifetime-probe'
    for mono,file in [(False,'generic.llbc'),(True,'mono.llbc')]:
        d=json.loads((base/file).read_text());print(file,json.dumps(audit(d,mono)))
        if not mono:negative_controls(d)
    print('Four invented/merged/lost lifetime and return-origin copied witnesses rejected.')
