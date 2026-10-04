#!/usr/bin/env python3
"""Lower the complete actual mem::replace LLBC body to restricted scalar heap IR.
Rust/LLBC/IR semantics and pointer validity correspondence are not proved.
"""
import argparse,copy,json,pathlib
from audit_jumpdest_llbc import ident


def lower(data):
 if data['has_errors']: raise ValueError('Extraction errors')
 x=data['translated'];fs=[f for f in x['fun_decls'] if f and ident(f)=='core::mem::replace']
 if len(fs)!=1 or not isinstance(fs[0]['body'],dict): raise ValueError('Missing actual mem::replace body')
 f=fs[0];sg=f['signature'];g=f['generics']
 if sg['is_unsafe'] or sg['abi']!='Rust' or sg['is_variadic'] or len(sg['inputs'])!=2 or len(g['types'])!=1 or len(g['regions'])!=1 or g['trait_clauses'] or g['const_generics']: raise ValueError('Generic signature changed')
 cache={};constant_cache={}
 def collect(v):
  if isinstance(v,dict):
   constant=v.get('Const')
   if isinstance(constant,dict) and 'Value' in constant:
    ci,cv=constant['Value']
    if ci in constant_cache and constant_cache[ci]!=cv:raise ValueError('Constant cache conflict')
    constant_cache[ci]=cv
   pair=v.get('Value')
   if isinstance(pair,list) and len(pair)==2 and isinstance(pair[1],dict) and len(pair[1])==1 and next(iter(pair[1])) in ['Ref','RawPtr','TypeVar','Adt']:
    if pair[0] in cache and cache[pair[0]]!=pair[1]: raise ValueError('Type cache conflict')
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
 generic={'TypeVar':{'Free':0}}
 reference=ty(sg['inputs'][0]).get('Ref')
 if not reference or reference[0]!={'Var':{'Free':0}} or reference[2]!='Mut' or ty(reference[1])!=generic or ty(sg['inputs'][1])!=generic or ty(sg['output'])!=generic:raise ValueError('Input/output type correspondence changed')
 def local(p):
  if set(p['kind'])!={'Local'}:raise ValueError('Expected local')
  return p['kind']['Local']
 def deref(p):
  pr=p['kind'].get('Projection')
  if not pr or pr[1]!='Deref':raise ValueError('Expected single dereference')
  return local(pr[0])
 ops=[]
 for s in f['body']['Structured']['body']['statements']:
  k=s['kind']
  if k=='Return': ops.append('.ret');continue
  if not isinstance(k,dict) or len(k)!=1:raise ValueError('Unsupported statement')
  tag=next(iter(k));v=k[tag]
  if tag in ['StorageLive','StorageDead']:
   ops.append(('.live ' if tag=='StorageLive' else '.dead ')+str(v));continue
  if tag!='Assign':raise ValueError('Unmodeled effect '+tag)
  dest,rhs=v
  if 'RawPtr' in rhs:
   r=rhs['RawPtr'];src=deref(r['place']);dst=local(dest)
   if r['kind'] not in ['Shared','Mut'] or ty(r['place']['ty'])!=generic or ty(dest['ty']).get('RawPtr',[None,None])[1]!=r['kind'] or ty(ty(dest['ty'])['RawPtr'][0])!=generic:raise ValueError('Raw address type/mode changed')
   # Thin unit pointer metadata only; no arbitrary metadata is discarded.
   meta=r['ptr_metadata']['Const'];value=meta['Value'][1] if 'Value' in meta else constant_cache.get(meta.get('Deduplicated'))
   if value is None or value[0]!={'Adt':[None,[]]}:raise ValueError('Non-unit pointer metadata')
   metadata_type=ty(value[1]).get('Adt')
   if metadata_type is None or metadata_type['builtin']!='Tuple' or any(metadata_type['generics'].values()):raise ValueError('Pointer metadata type changed')
   ops.append(f'.address .{ "shared" if r["kind"]=="Shared" else "mutable"} {dst} {src}')
  elif 'Use' in rhs:
   operand,retag=rhs['Use']
   if retag!='Yes':raise ValueError('Retag metadata changed')
   if set(operand)=={'Copy'}:
    src=deref(operand['Copy']);dst=local(dest)
    if ty(dest['ty'])!=generic or ty(operand['Copy']['ty'])!=generic:raise ValueError('Load type changed')
    ops.append(f'.read {dst} {src}')
   elif set(operand)=={'Move'}:
    src=local(operand['Move'])
    if ty(operand['Move']['ty'])!=generic or ty(dest['ty'])!=generic:raise ValueError('Move type changed')
    if 'Projection' in dest['kind']:ops.append(f'.write {deref(dest)} {src}')
    else:ops.append(f'.move {local(dest)} {src}')
   else:raise ValueError('Unsupported operand')
  else:raise ValueError('Unsupported rvalue')
 if len(ops)!=15 or ops[-1]!='.ret':raise ValueError('Actual operation inventory changed')
 return ops


def render(ops):
 return 'import StdMemReplaceState\n-- Generated from the complete fixed actual mem::replace LLBC body.\n-- Scalar-instance heap IR only; Rust pointer/retag correspondence is unproved.\nnamespace StdMemReplaceState\ndef actualMemReplace : List Instruction := [\n  '+',\n  '.join(ops)+'\n]\nend StdMemReplaceState\n'


def negative_controls(data):
 for label in ['signature','mode','metadata','metadata_type','retag','load_type','call','return']:
  bad=copy.deepcopy(data);f=next(f for f in bad['translated']['fun_decls'] if f and ident(f)=='core::mem::replace');ss=f['body']['Structured']['body']['statements'];assigns=[s['kind']['Assign'] for s in ss if isinstance(s['kind'],dict) and 'Assign' in s['kind']]
  if label=='signature':f['signature']['abi']='C'
  elif label=='mode':assigns[0][1]['RawPtr']['kind']='Mut'
  elif label=='metadata':assigns[0][1]['RawPtr']['ptr_metadata']['Const']={'Value':[99999,[{'Integer':{'Unsigned':['Usize','1']}},{'Deduplicated':6}]]}
  elif label=='metadata_type':assigns[0][1]['RawPtr']['ptr_metadata']['Const']={'Value':[99999,[{'Adt':[None,[]]},{'Value':[99999,{'RawPtr':[{'Deduplicated':10},'Shared']}]}]]}
  elif label=='retag':assigns[1][1]['Use'][1]='No'
  elif label=='load_type':assigns[1][0]['ty']=copy.deepcopy(assigns[0][0]['ty'])
  elif label=='call':ss[0]['kind']={'Call':{}}
  else:ss[-1]['kind']='UnwindResume'
  try:lower(bad)
  except ValueError:pass
  else:raise AssertionError('Changed source schema accepted: '+label)

def drift_controls(data):
 expected=render(lower(data))
 for mode in ['dead_pointer_write','different_load_local']:
  bad=copy.deepcopy(data);f=next(f for f in bad['translated']['fun_decls'] if f and ident(f)=='core::mem::replace');assigns=[s['kind']['Assign'] for s in f['body']['Structured']['body']['statements'] if isinstance(s['kind'],dict) and 'Assign' in s['kind']]
  if mode=='dead_pointer_write':assigns[3][0]['kind']['Projection'][0]['kind']['Local']=4
  else:assigns[1][0]['kind']['Local']=6
  assert render(lower(bad))!=expected,'Changed supported operands omitted: '+mode

if __name__=='__main__':
 ap=argparse.ArgumentParser();ap.add_argument('llbc');ap.add_argument('--output',required=True);ap.add_argument('--check',action='store_true');args=ap.parse_args()
 data=json.loads(pathlib.Path(args.llbc).read_text());out=render(lower(data));negative_controls(data);drift_controls(data);p=pathlib.Path(args.output)
 if args.check:
  if p.read_text()!=out:raise SystemExit('Generated scalar heap IR drift')
 else:p.write_text(out)
 print('Complete actual mem::replace body lowered; eight altered schemas rejected and two supported operand changes retained; Rust/heap correspondence remains unproved.')
