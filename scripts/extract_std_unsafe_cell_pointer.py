#!/usr/bin/env python3
"""Actual UnsafeCell::get scalar pointer IR; Rust layout/cast refinement unproved."""
import copy,json,pathlib,argparse
from audit_jumpdest_llbc import ident


def lower(data):
 if data['has_errors']:raise ValueError('Extraction errors')
 x=data['translated'];cache={};constants={}
 def collect(v):
  if isinstance(v,dict):
   constant=v.get('Const')
   if isinstance(constant,dict) and 'Value' in constant:
    ci,cv=constant['Value']
    if ci in constants and constants[ci]!=cv:raise ValueError('Constant cache conflict')
    constants[ci]=cv
   pair=v.get('Value')
   if isinstance(pair,list) and len(pair)==2 and isinstance(pair[1],dict) and len(pair[1])==1 and next(iter(pair[1])) in ['Ref','RawPtr','TypeVar','Adt']:
    if pair[0] in cache and cache[pair[0]]!=pair[1]:raise ValueError('Type cache conflict')
    cache[pair[0]]=pair[1]
   for a in v.values():collect(a)
  elif isinstance(v,list):
   for a in v:collect(a)
 collect(x)
 def ty(t):
  if 'Deduplicated' in t:
   if t['Deduplicated'] not in cache:raise ValueError('Missing type cache')
   return cache[t['Deduplicated']]
  return t['Value'][1]
 cell=next(t for t in x['type_decls'] if t and ident(t)=='core::cell::UnsafeCell')
 if cell['item_meta']['lang_item']!='UnsafeCell' or cell['ptr_metadata']!='None' or cell['generics']['regions'] or len(cell['generics']['types'])!=1 or not isinstance(cell['kind'],dict) or len(cell['kind']['Struct'])!=1 or cell['kind']['Struct'][0]['name']!='value' or ty(cell['kind']['Struct'][0]['ty'])!={'TypeVar':{'Free':0}}:raise ValueError('Actual UnsafeCell layout/type changed')
 layouts=cell['layout']
 if len(layouts)!=1 or layouts[0]['key']!='aarch64-apple-darwin' or layouts[0]['value']['repr']!={'repr_algo':'Rust','align_modif':None,'transparent':True,'explicit_discr_type':None} or len(layouts[0]['value']['variant_layouts'])!=1 or len(layouts[0]['value']['variant_layouts'][0]['field_offsets'])!=1:raise ValueError('Transparent layout metadata changed')
 # repr(transparent) is recorded, not turned into an unconditional physical layout theorem.
 fs=[f for f in x['fun_decls'] if f and ident(f)=='core::cell::get' and isinstance(f['body'],dict)]
 fs=[f for f in fs if ty(ty(f['signature']['inputs'][0])['Ref'][1]).get('Adt',{}).get('id')==cell['def_id']]
 if len(fs)!=1:raise ValueError('Missing actual UnsafeCell::get body')
 f=fs[0];sg=f['signature'];g=f['generics'];reference=ty(sg['inputs'][0])['Ref']
 if sg['is_unsafe'] or sg['abi']!='Rust' or sg['is_variadic'] or len(sg['inputs'])!=1 or reference[0]!={'Var':{'Free':0}} or reference[2]!='Shared' or len(g['regions'])!=1 or len(g['types'])!=1 or g['trait_clauses'] or g['const_generics']:raise ValueError('Actual get signature changed')
 def shape(t):
  raw=ty(t).get('RawPtr')
  if not raw:raise ValueError('Expected typed raw pointer')
  pointee=ty(raw[0])
  if pointee=={'TypeVar':{'Free':0}}:name='scalar'
  elif pointee.get('Adt',{}).get('id')==cell['def_id'] and [ty(t) for t in pointee['Adt']['generics']['types']]==[{'TypeVar':{'Free':0}}]:name='cell'
  else:raise ValueError('Unrelated cast pointee')
  return name,raw[1]
 if shape(sg['output'])!=('scalar','Mut'):raise ValueError('Return pointer changed')
 def local(p):
  if set(p['kind'])!={'Local'}:raise ValueError('Expected local')
  return p['kind']['Local']
 ops=[];casts=[]
 for s in f['body']['Structured']['body']['statements']:
  k=s['kind']
  if k=='Return':ops.append('.ret');continue
  if not isinstance(k,dict) or len(k)!=1:raise ValueError('Unsupported statement')
  tag=next(iter(k));v=k[tag]
  if tag in ['StorageLive','StorageDead']:ops.append(('.live ' if tag=='StorageLive' else '.dead ')+str(v));continue
  if tag!='Assign':raise ValueError('Unmodeled effect '+tag)
  dst,rhs=v;di=local(dst)
  if 'RawPtr' in rhs:
   r=rhs['RawPtr'];pr=r['place']['kind'].get('Projection')
   if not pr or pr[1]!='Deref' or r['kind']!='Shared' or shape(dst['ty'])!=('cell','Shared'):raise ValueError('Raw address origin/mode changed')
   if ty(r['place']['ty']).get('Adt',{}).get('id')!=cell['def_id']:raise ValueError('Raw address pointee changed')
   meta=r['ptr_metadata']['Const'];cv=meta['Value'][1] if 'Value' in meta else constants.get(meta.get('Deduplicated'))
   if cv is None or cv[0]!={'Adt':[None,[]]} or ty(cv[1]).get('Adt',{}).get('builtin')!='Tuple' or any(ty(cv[1])['Adt']['generics'].values()):raise ValueError('Unit pointer metadata changed')
   ops.append(f'.address {di} {local(pr[0])}')
  elif 'UnaryOp' in rhs:
   op,operand=rhs['UnaryOp']
   if set(op)!={'Cast'} or set(op['Cast'])!={'RawPtr'} or set(operand)!={'Move'}:raise ValueError('Non-pointer cast')
   src,dest=op['Cast']['RawPtr'];a,b=shape(src),shape(dest)
   if shape(operand['Move']['ty'])!=a or shape(dst['ty'])!=b:raise ValueError('Cast operand/result type changed')
   if (a,b)==(('cell','Shared'),('scalar','Shared')):ci='cellToScalar'
   elif (a,b)==(('scalar','Shared'),('scalar','Mut')):ci='constToMutable'
   else:raise ValueError('Unexpected pointer cast')
   casts.append(ci);ops.append(f'.cast .{ci} {di} {local(operand["Move"])}')
  elif 'Use' in rhs:
   operand,retag=rhs['Use']
   if set(operand)!={'Copy'} or retag!='Yes' or shape(operand['Copy']['ty'])!=shape(dst['ty']):raise ValueError('Pointer copy/retag changed')
   ops.append(f'.copy {di} {local(operand["Copy"])}')
  else:raise ValueError('Unsupported rvalue')
 if len(ops)!=19 or casts!=['cellToScalar','constToMutable'] or ops[-1]!='.ret':raise ValueError('Whole get operation inventory changed')
 return ops


def render(ops):
 return 'import StdUnsafeCellPointer\n-- Actual fixed UnsafeCell::get LLBC, scalar-instance pointer IR.\n-- repr(transparent) metadata audited; physical cast/layout correspondence unproved.\nnamespace StdUnsafeCellPointer\ndef actualUnsafeCellGet : List Instruction := [\n  '+',\n  '.join(ops)+'\n]\nend StdUnsafeCellPointer\n'


def negative_controls(data):
 for label in ['repr','field','abi','cast_kind','cast_type','retag','return']:
  bad=copy.deepcopy(data);x=bad['translated'];cell=next(t for t in x['type_decls'] if t and ident(t)=='core::cell::UnsafeCell');f=next(f for f in x['fun_decls'] if f and ident(f)=='core::cell::get' and isinstance(f['body'],dict) and any(isinstance(s['kind'],dict) and 'Assign' in s['kind'] and 'UnaryOp' in s['kind']['Assign'][1] for s in f['body']['Structured']['body']['statements']));ss=f['body']['Structured']['body']['statements'];assigns=[s['kind']['Assign'] for s in ss if isinstance(s['kind'],dict) and 'Assign' in s['kind']]
  if label=='repr':cell['layout'][0]['value']['repr']['transparent']=False
  elif label=='field':cell['kind']['Struct'][0]['name']='other'
  elif label=='abi':f['signature']['abi']='C'
  elif label=='cast_kind':assigns[2][1]['UnaryOp'][0]={'Cast':{'Scalar':[]}}
  elif label=='cast_type':assigns[2][1]['UnaryOp'][0]['Cast']['RawPtr'][1]=copy.deepcopy(assigns[0][0]['ty'])
  elif label=='retag':assigns[1][1]['Use'][1]='No'
  else:ss[-1]['kind']='UnwindResume'
  try:lower(bad)
  except ValueError:pass
  else:raise AssertionError('Changed UnsafeCell witness accepted: '+label)

if __name__=='__main__':
 ap=argparse.ArgumentParser();ap.add_argument('llbc');ap.add_argument('--output',required=True);ap.add_argument('--check',action='store_true');a=ap.parse_args();data=json.loads(pathlib.Path(a.llbc).read_text());text=render(lower(data));negative_controls(data);out=pathlib.Path(a.output)
 if a.check:
  if out.read_text()!=text:raise SystemExit('Actual pointer IR drift')
 else:out.write_text(text)
 print('Actual UnsafeCell transparent layout/cast chain and nineteen statements inspected; seven copied witness changes rejected.')
