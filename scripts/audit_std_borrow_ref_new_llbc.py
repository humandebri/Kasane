#!/usr/bin/env python3
"""Audit actual BorrowRef::new and Cell extraction; no heap/refinement proof."""
import copy
import hashlib
import json
from audit_jumpdest_llbc import ident


def inspect(data, transparent_cell):
    if data['has_errors']:
        raise ValueError('Extraction errors')
    x = data['translated']
    cache = {}
    def collect(v):
        if isinstance(v, dict):
            item = v.get('Value')
            if isinstance(item, list) and len(item) == 2 and isinstance(item[1], dict) and len(item[1]) == 1 and next(iter(item[1])) in ['Adt', 'Ref', 'Scalar', 'TypeVar', 'RawPtr']:
                if item[0] in cache and cache[item[0]] != item[1]:
                    raise ValueError('Conflicting type cache')
                cache[item[0]] = item[1]
            for a in v.values(): collect(a)
        elif isinstance(v, list):
            for a in v: collect(a)
    collect(x)
    def resolve(t):
        if 'Deduplicated' in t:
            if t['Deduplicated'] not in cache: raise ValueError('Missing type cache')
            return cache[t['Deduplicated']]
        return t['Value'][1]
    fs = [f for f in x['fun_decls'] if f and isinstance(f['body'], dict)]
    names = sorted(ident(f) for f in fs)
    expected = ['core::cell::UNUSED', 'core::cell::is_reading', 'core::cell::new']
    if transparent_cell: expected += ['core::cell::get', 'core::cell::get', 'core::cell::replace']
    if names != sorted(expected): raise ValueError('Actual body inventory changed')
    f = next(f for f in fs if ident(f) == 'core::cell::new')
    sg = f['signature']; g = f['generics']
    if sg['is_unsafe'] or sg['abi'] != 'Rust' or sg['is_variadic'] or len(sg['inputs']) != 1 or len(g['regions']) != 1 or any(v for k,v in g.items() if k != 'regions'):
        raise ValueError('BorrowRef signature changed')
    receiver = resolve(sg['inputs'][0])['Ref']
    if receiver[0] != {'Var': {'Free': 0}} or receiver[2] != 'Shared': raise ValueError('Receiver lifetime changed')
    cell = resolve(receiver[1])['Adt']
    if ident(x['type_decls'][cell['id']]) != 'core::cell::Cell' or resolve(cell['generics']['types'][0]) != {'Scalar': {'Integer': {'Signed': 'Isize'}}}:
        raise ValueError('Counter cell type changed')
    option = resolve(sg['output'])['Adt']; borrow = resolve(option['generics']['types'][0])['Adt']
    if ident(x['type_decls'][option['id']]) != 'core::option::Option' or ident(x['type_decls'][borrow['id']]) != 'core::cell::BorrowRef' or borrow['generics']['regions'] != [{'Var': {'Free': 0}}]:
        raise ValueError('Returned token lifetime changed')
    statements = f['body']['Structured']['body']['statements']
    calls = [s['kind']['Call']['call'] for s in statements if isinstance(s['kind'],dict) and 'Call' in s['kind']]
    if [ident(x['fun_decls'][c['func']['Regular']['kind']['Fun']]) for c in calls] != ['core::cell::get','core::num::wrapping_add','core::cell::is_reading','core::cell::replace']:
        raise ValueError('Call order changed')
    if [c['dest']['kind'] for c in calls] != [{'Local':3},{'Local':2},{'Local':5},{'Local':7}]: raise ValueError('Call destinations changed')
    if calls[1]['args'][0]['Move']['kind'] != {'Local':3} or calls[1]['args'][1]['Const']['Value'][1][0] != {'Integer': {'Signed':['Isize','1']}}: raise ValueError('Wrapping increment changed')
    if [a['Move']['kind'] for a in calls[3]['args']] != [{'Local':8},{'Local':9}]: raise ValueError('Cell replace arguments changed')
    sw = next(s['kind']['Switch'] for s in statements if isinstance(s['kind'],dict) and 'Switch' in s['kind'])
    if sw['data']['scrutinee']['Value']['Move']['kind'] != {'Local':5} or sw['data']['branches'][0][0]['Value'][1][0] != {'Bool':False} or sw['data']['branches'][0][1] != 1 or sw['data']['fallback'] != 0:
        raise ValueError('Read guard polarity changed')
    reject = sw['branches'][1]['statements']
    assignment = next(s['kind']['Assign'] for s in reject if isinstance(s['kind'],dict) and 'Assign' in s['kind'])
    if assignment[0]['kind'] != {'Local':0} or assignment[1]['Aggregate'][0]['Adt'][1] != 0 or assignment[1]['Aggregate'][1] != [] or reject[-1]['kind'] != 'Return':
        raise ValueError('Reject result changed')
    if sw['branches'][0]['statements'] or statements[-1]['kind'] != 'Return': raise ValueError('Branch return changed')
    for name in ['Cell','UnsafeCell']:
        matches = [t for t in x['type_decls'] if t and ident(t) == 'core::cell::'+name]
        if transparent_cell:
            if len(matches)!=1 or not isinstance(matches[0]['kind'],dict) or len(matches[0]['kind']['Struct'])!=1 or matches[0]['kind']['Struct'][0]['name']!='value': raise ValueError('Transparent cell layout changed')
        elif name=='Cell' and (len(matches)!=1 or matches[0]['kind']!='Opaque'): raise ValueError('Opaque Cell scope changed')
    def canonical(v):
        if isinstance(v,dict): return {k:canonical(a) for k,a in v.items() if k not in ['span','comments_before'] and not (k=='id' and 'span' in v and 'kind' in v)}
        if isinstance(v,list): return [canonical(a) for a in v]
        return v
    shape=hashlib.sha256(json.dumps([canonical(f['body']) for f in fs],sort_keys=True).encode()).hexdigest()
    return dict(transparent_functions=len(fs),transparent_cell=transparent_cell,shared_receiver_region=0,returned_token_region=0,increment=1,rejection_bool=False,body_shape_sha256=shape,semantic_equivalence_proved=False)


def negative_controls(data, transparent_cell):
    for label in ['increment','guard','wrong_replace_arg','reject_some','token_lifetime','return']:
        bad=copy.deepcopy(data);x=bad['translated'];f=next(f for f in x['fun_decls'] if f and ident(f)=='core::cell::new');ss=f['body']['Structured']['body']['statements']
        calls=[s['kind']['Call']['call'] for s in ss if isinstance(s['kind'],dict) and 'Call' in s['kind']]
        sw=next(s['kind']['Switch'] for s in ss if isinstance(s['kind'],dict) and 'Switch' in s['kind'])
        if label=='increment': calls[1]['args'][1]['Const']['Value'][1][0]['Integer']['Signed'][1]='2'
        elif label=='guard': sw['data']['branches'][0][0]['Value'][1][0]['Bool']=True
        elif label=='wrong_replace_arg': calls[3]['args'][1]['Move']['kind']={'Local':3}
        elif label=='reject_some': next(s['kind']['Assign'] for s in sw['branches'][1]['statements'] if isinstance(s['kind'],dict) and 'Assign' in s['kind'])[1]['Aggregate'][0]['Adt'][1]=1
        elif label=='token_lifetime': f['signature']['output']['Value'][1]['Adt']['generics']['types'][0]['Value'][1]['Adt']['generics']['regions']=[{'Erased':None}]
        else: ss[-1]['kind']='UnwindResume'
        try: inspect(bad,transparent_cell)
        except ValueError: pass
        else: raise AssertionError('Changed actual BorrowRef witness accepted: '+label)
