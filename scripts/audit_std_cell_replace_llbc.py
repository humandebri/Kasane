#!/usr/bin/env python3
"""Audit complete actual Cell::replace main/unwind ownership protocol.
This is structural source evidence, not Rust/heap/drop semantic proof.
"""
import copy,hashlib,json
from audit_jumpdest_llbc import ident
from extract_std_cell_get_state import lower as lower_get
from extract_std_mem_replace_state import lower as lower_mem


def inspect(data):
 lower_get(data);lower_mem(data)
 x=data['translated'];types={};constants={};traits={}
 def collect(v):
  if isinstance(v,dict):
   pair=v.get('Value')
   if isinstance(pair,list) and len(pair)==2 and isinstance(pair[1],dict):
    item=pair[1]
    target=types if len(item)==1 and next(iter(item)) in ['Ref','RawPtr','Adt','Scalar','TypeVar'] else traits if 'trait_decl_ref' in item and 'kind' in item else None
    if target is not None:
     if pair[0] in target and target[pair[0]]!=item:raise ValueError('Conflicting cache')
     target[pair[0]]=item
   if isinstance(pair,list) and len(pair)==2 and isinstance(pair[1],list) and len(pair[1])==2 and isinstance(pair[1][0],dict) and len(pair[1][0])==1 and next(iter(pair[1][0])) in ['Bool','Integer','Adt','SizeOf','AlignOf']:
    if pair[0] in constants and constants[pair[0]]!=pair[1]:raise ValueError('Conflicting literal cache')
    constants[pair[0]]=pair[1]
   const=v.get('Const')
   if isinstance(const,dict) and 'Value' in const:
    ci,cv=const['Value']
    if ci in constants and constants[ci]!=cv:raise ValueError('Conflicting constant cache')
    constants[ci]=cv
   for a in v.values():collect(a)
  elif isinstance(v,list):
   for a in v:collect(a)
 collect(x)
 def resolve(v,cache):
  if 'Deduplicated' in v:
   if v['Deduplicated'] not in cache:raise ValueError('Missing cache entry')
   return cache[v['Deduplicated']]
  return v['Value'][1]
 def ty(t):return resolve(t,types)
 def boolean(c):
  value=resolve(c,constants)
  if ty(value[1])!={'Scalar':'Bool'} or set(value[0])!={'Bool'}:raise ValueError('Not a boolean flag')
  return value[0]['Bool']
 generic={'TypeVar':{'Free':0}}
 cell=next(t for t in x['type_decls'] if t and ident(t)=='core::cell::Cell')
 unsafe=next(t for t in x['type_decls'] if t and ident(t)=='core::cell::UnsafeCell')
 fs=[f for f in x['fun_decls'] if f and ident(f)=='core::cell::replace' and isinstance(f['body'],dict)]
 if len(fs)!=1:raise ValueError('Missing actual Cell::replace')
 f=fs[0];sg=f['signature'];receiver=ty(sg['inputs'][0]).get('Ref')
 if sg['is_unsafe'] or sg['abi']!='Rust' or sg['is_variadic'] or len(sg['inputs'])!=2 or not receiver or receiver[0]!={'Var':{'Free':0}} or receiver[2]!='Shared' or ty(receiver[1]).get('Adt',{}).get('id')!=cell['def_id'] or ty(sg['inputs'][1])!=generic or ty(sg['output'])!=generic:raise ValueError('Actual replace signature changed')
 ss=f['body']['Structured']['body']['statements']
 tags=[next(iter(s['kind'])) if isinstance(s['kind'],dict) else s['kind'] for s in ss]
 if tags!=['StorageLive','StorageLive','Assign','Assign','StorageLive','StorageLive','StorageLive','StorageLive','StorageLive','Assign','Call','StorageDead','Assign','Assign','StorageDead','StorageDead','Assign','StorageLive','Assign','Assign','Call','StorageDead','StorageDead','StorageDead','StorageDead','StorageDead','StorageDead','Return']:raise ValueError('Main operation inventory changed')
 def local(p):
  if set(p['kind'])!={'Local'}:raise ValueError('Expected local')
  return p['kind']['Local']
 def flag(index,expected):
  lhs,rhs=ss[index]['kind']['Assign']
  if local(lhs)!=9 or ty(lhs['ty'])!={'Scalar':'Bool'} or rhs['Use'][1]!='Yes' or boolean(rhs['Use'][0]['Const'])!=expected:raise ValueError('Drop flag transition changed')
 flag(2,False);flag(3,True);flag(18,False)
 field=ss[9]['kind']['Assign'];pr=field[1]['Ref']['place']['kind'].get('Projection')
 if local(field[0])!=7 or field[1]['Ref']['kind']!='Shared' or not pr or pr[1]!={'Field':[None,0]} or pr[0]['kind']['Projection'][1]!='Deref' or local(pr[0]['kind']['Projection'][0])!=1 or ty(field[1]['Ref']['place']['ty']).get('Adt',{}).get('id')!=unsafe['def_id']:raise ValueError('Field origin changed')
 calls=[ss[10]['kind']['Call'],ss[20]['kind']['Call']]
 for call,callee_name,dest,args in zip(calls,['core::cell::get','core::mem::replace'],[6,0],[[7],[3,8]]):
  c=call['call'];fid=c['func']['Regular']['kind']['Fun'];callee=x['fun_decls'][fid]
  if ident(callee)!=callee_name or local(c['dest'])!=dest or [local(a['Move']) for a in c['args']]!=args or c['safety']!='Inherit' or [ty(t) for t in c['func']['Regular']['generics']['types']]!=[generic] or c['func']['Regular']['generics']['trait_refs']:raise ValueError('Actual call/arguments/type substitution changed')
  if callee_name.endswith('::get') and ty(ty(callee['signature']['inputs'][0])['Ref'][1]).get('Adt',{}).get('id')!=unsafe['def_id']:raise ValueError('Wrong get implementation')
 for index,dest,src,kind in [(12,5,6,'Mut'),(13,4,5,'Mut'),(16,3,4,'TwoPhaseMut')]:
  lhs,rhs=ss[index]['kind']['Assign'];r=rhs['Ref'];p=r['place']['kind'].get('Projection')
  if local(lhs)!=dest or not p or p[1]!='Deref' or local(p[0])!=src or r['kind']!=kind or ty(r['place']['ty'])!=generic:raise ValueError('Mutable reborrow chain changed')
 lhs,rhs=ss[19]['kind']['Assign']
 if local(lhs)!=8 or rhs['Use'][1]!='Yes' or local(rhs['Use'][0]['Move'])!=2 or ty(lhs['ty'])!=generic:raise ValueError('Owned value move changed')
 cleanup_count=0
 for call in calls:
  cleanup=call['on_unwind']['statements']
  if len(cleanup)!=5 or [s['kind'] for s in cleanup[1:]]!=[{'StorageDead':9},{'StorageDead':2},{'StorageDead':1},'UnwindResume']:raise ValueError('Outer unwind cleanup changed')
  sw=cleanup[0]['kind']['Switch']
  if local(sw['data']['scrutinee']['Value']['Copy'])!=9 or len(sw['data']['branches'])!=1 or boolean(sw['data']['branches'][0][0]) is not False or sw['data']['branches'][0][1]!=1 or sw['data']['fallback']!=0 or len(sw['branches'])!=2 or sw['branches'][1]['statements']:raise ValueError('Drop condition polarity changed')
  dropped=sw['branches'][0]['statements']
  if len(dropped)!=1 or 'Drop' not in dropped[0]['kind']:raise ValueError('Drop branch changed')
  d=dropped[0]['kind']['Drop'];tr,method=d['fn_ptr']['kind']['Trait'];tr=resolve(tr,traits)
  if local(d['place'])!=2 or ty(d['place']['ty'])!=generic or d['kind']!='Precise' or method!=0 or tr['kind'].get('BuiltinOrAuto',{}).get('builtin_data')!='UntrackedDestruct':raise ValueError('Owned value destructor changed')
  trait=x['trait_decls'][tr['trait_decl_ref']['skip_binder']['id']]
  if ident(trait)!='core::marker::Destruct' or [ty(t) for t in tr['trait_decl_ref']['skip_binder']['generics']['types']]!=[generic]:raise ValueError('Destructor trait substitution changed')
  if [s['kind'] for s in d['on_unwind']['statements']]!=[{'StorageDead':9},{'StorageDead':2},{'StorageDead':1},'UnwindTerminate']:raise ValueError('Destructor double-unwind behavior changed')
  cleanup_count+=5+1+4
 def canonical(v):
  if isinstance(v,dict):return {k:canonical(a) for k,a in v.items() if k not in ['span','comments_before'] and not(k=='id' and 'span' in v and 'kind' in v)}
  if isinstance(v,list):return [canonical(a) for a in v]
  return v
 shape=hashlib.sha256(json.dumps(canonical(f['body']),sort_keys=True).encode()).hexdigest()
 return dict(main_statements=28,nested_cleanup_statements=cleanup_count,first_call='UnsafeCell::get',second_call='mem::replace',flag_before_first_call=True,flag_cleared_before_value_move=True,moved_value_source=2,moved_value_destination=8,cleanup_drop_local=2,drop_only_when_flag_true=True,destructor_unwind='UnwindTerminate',body_shape_sha256=shape,semantic_equivalence_proved=False)


def negative_controls(data):
 for label in ['first_flag','late_clear','move_origin','reborrow','callee','call_value','drop_polarity','drop_local','drop_kind','double_unwind','return']:
  bad=copy.deepcopy(data);f=next(f for f in bad['translated']['fun_decls'] if f and ident(f)=='core::cell::replace' and isinstance(f['body'],dict));ss=f['body']['Structured']['body']['statements'];sw=ss[10]['kind']['Call']['on_unwind']['statements'][0]['kind']['Switch'];drop=sw['branches'][0]['statements'][0]['kind']['Drop']
  if label=='first_flag':ss[3]['kind']['Assign'][1]['Use'][0]['Const']={'Value':[99999,[{'Bool':False},copy.deepcopy(ss[3]['kind']['Assign'][0]['ty'])]]}
  elif label=='late_clear':ss[18],ss[19]=ss[19],ss[18]
  elif label=='move_origin':ss[19]['kind']['Assign'][1]['Use'][0]['Move']['kind']={'Local':1}
  elif label=='reborrow':ss[16]['kind']['Assign'][1]['Ref']['kind']='Shared'
  elif label=='callee':ss[10]['kind']['Call']['call']['func']['Regular']['kind']['Fun']=f['def_id']
  elif label=='call_value':ss[20]['kind']['Call']['call']['args'][1]['Move']['kind']={'Local':2}
  elif label=='drop_polarity':sw['data']['branches'][0][0]={'Value':[99999,[{'Bool':True},copy.deepcopy(ss[3]['kind']['Assign'][0]['ty'])]]}
  elif label=='drop_local':drop['place']['kind']={'Local':8}
  elif label=='drop_kind':drop['kind']='Shallow'
  elif label=='double_unwind':drop['on_unwind']['statements'][-1]['kind']='UnwindResume'
  else:ss[-1]['kind']='UnwindResume'
  try:inspect(bad)
  except ValueError:pass
  else:raise AssertionError('Altered Cell replace witness accepted: '+label)
