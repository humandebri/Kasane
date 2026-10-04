#!/usr/bin/env python3
"""Actual Cell::get scalar state IR, composed with actual UnsafeCell pointer IR.
Rust layout/ref/retag/read/unwind and extractor refinement remain unproved.
"""
import argparse,copy,json,pathlib
from audit_jumpdest_llbc import ident
from extract_std_unsafe_cell_pointer import lower as lower_unsafe


def lower(data):
 lower_unsafe(data)  # Validate the actual called implementation and layout metadata too.
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
 cell=next(t for t in x['type_decls'] if t and ident(t)=='core::cell::Cell');unsafe=next(t for t in x['type_decls'] if t and ident(t)=='core::cell::UnsafeCell')
 if not isinstance(cell['kind'],dict) or len(cell['kind']['Struct'])!=1 or cell['kind']['Struct'][0]['name']!='value' or ty(cell['kind']['Struct'][0]['ty']).get('Adt',{}).get('id')!=unsafe['def_id'] or len(cell['layout'])!=1 or cell['layout'][0]['key']!='aarch64-apple-darwin' or cell['layout'][0]['value']['repr']!={'repr_algo':'Rust','align_modif':None,'transparent':True,'explicit_discr_type':None}:raise ValueError('Cell field/layout changed')
 generic={'TypeVar':{'Free':0}}
 if [ty(t) for t in ty(cell['kind']['Struct'][0]['ty'])['Adt']['generics']['types']]!=[generic]:raise ValueError('Cell field type substitution changed')
 fs=[f for f in x['fun_decls'] if f and ident(f)=='core::cell::get' and isinstance(f['body'],dict) and ty(ty(f['signature']['inputs'][0])['Ref'][1]).get('Adt',{}).get('id')==cell['def_id']]
 if len(fs)!=1:raise ValueError('Missing actual Cell::get')
 f=fs[0];sg=f['signature'];receiver=ty(sg['inputs'][0])['Ref']
 if sg['is_unsafe'] or sg['abi']!='Rust' or sg['is_variadic'] or len(sg['inputs'])!=1 or receiver[0]!={'Var':{'Free':0}} or receiver[2]!='Shared' or ty(sg['output'])!=generic:raise ValueError('Cell::get signature changed')
 def local(p):
  if set(p['kind'])!={'Local'}:raise ValueError('Expected local')
  return p['kind']['Local']
 def deref(p):
  pr=p['kind'].get('Projection')
  if not pr or pr[1]!='Deref':raise ValueError('Expected pointer dereference')
  return local(pr[0])
 ops=[]
 for s in f['body']['Structured']['body']['statements']:
  k=s['kind']
  if k=='Return':ops.append('.ret');continue
  if not isinstance(k,dict) or len(k)!=1:raise ValueError('Unmodeled statement')
  tag=next(iter(k));v=k[tag]
  if tag in ['StorageLive','StorageDead']:ops.append(('.live ' if tag=='StorageLive' else '.dead ')+str(v));continue
  if tag=='Call':
   call=v['call'];fid=call['func']['Regular']['kind']['Fun'];callee=x['fun_decls'][fid]
   cr=ty(callee['signature']['inputs'][0])['Ref']
   if ident(callee)!='core::cell::get' or ty(cr[1]).get('Adt',{}).get('id')!=unsafe['def_id'] or len(call['args'])!=1 or call['safety']!='Inherit' or [ty(t) for t in call['func']['Regular']['generics']['types']]!=[generic] or call['func']['Regular']['generics']['trait_refs']:raise ValueError('Wrong actual callee/type substitution')
   uw=[z['kind'] for z in v['on_unwind']['statements']]
   if uw!=[{'StorageDead':1},'UnwindResume']:raise ValueError('Unwind cleanup changed')
   ctype=ty(call['args'][0]['Move']['ty']).get('Ref');dtype=ty(call['dest']['ty']).get('RawPtr')
   if not ctype or ctype[2]!='Shared' or ty(ctype[1]).get('Adt',{}).get('id')!=unsafe['def_id'] or [ty(t) for t in ty(ctype[1])['Adt']['generics']['types']]!=[generic] or not dtype or dtype[1]!='Mut' or ty(dtype[0])!=generic:raise ValueError('Call operand/result types changed')
   ops.append(f'.unsafeGet {local(call["dest"])} {local(call["args"][0]["Move"])}');continue
  if tag!='Assign':raise ValueError('Unsupported effect')
  dest,rhs=v
  if 'Ref' in rhs:
   r=rhs['Ref'];pr=r['place']['kind'].get('Projection')
   if r['kind']!='Shared' or not pr or pr[1]!={'Field':[None,0]} or ty(r['place']['ty']).get('Adt',{}).get('id')!=unsafe['def_id'] or ty(dest['ty']).get('Ref',[None,None,None])[2]!='Shared':raise ValueError('Field reference changed')
   meta=r['ptr_metadata']['Const'];cv=meta['Value'][1] if 'Value' in meta else constants.get(meta.get('Deduplicated'))
   if cv is None or cv[0]!={'Adt':[None,[]]} or ty(cv[1]).get('Adt',{}).get('builtin')!='Tuple' or any(ty(cv[1])['Adt']['generics'].values()):raise ValueError('Field reference metadata changed')
   src=deref(pr[0]);ops.append(f'.project {local(dest)} {src}')
  elif 'Use' in rhs:
   operand,retag=rhs['Use']
   if set(operand)!={'Copy'} or retag!='Yes' or ty(dest['ty'])!=generic or ty(operand['Copy']['ty'])!=generic:raise ValueError('Scalar raw read changed')
   raw=ty(operand['Copy']['kind']['Projection'][0]['ty']).get('RawPtr')
   if not raw or raw[1]!='Mut' or ty(raw[0])!=generic:raise ValueError('Read pointer type changed')
   ops.append(f'.read {local(dest)} {deref(operand["Copy"])}')
  else:raise ValueError('Unsupported rvalue')
 if len(ops)!=10 or ops[-1]!='.ret':raise ValueError('Whole main operation inventory changed')
 return ops


def render(ops):
 return 'import StdCellGetState\n-- Actual Cell::get main body; exact two-statement unwind cleanup audited.\n-- Scalar IR fault propagation abstracts Rust unwinding; physical refinement unproved.\nnamespace StdCellGetState\ndef actualCellGet : List Instruction := [\n  '+',\n  '.join(ops)+'\n]\nend StdCellGetState\n'


def negative_controls(data):
 for label in ['repr','field','callee','argument','projection','metadata','retag','unwind','return']:
  bad=copy.deepcopy(data);x=bad['translated'];cell=next(t for t in x['type_decls'] if t and ident(t)=='core::cell::Cell');f=next(f for f in x['fun_decls'] if f and ident(f)=='core::cell::get' and isinstance(f['body'],dict) and any(isinstance(s['kind'],dict) and 'Call' in s['kind'] for s in f['body']['Structured']['body']['statements']));ss=f['body']['Structured']['body']['statements'];call=next(s['kind']['Call'] for s in ss if isinstance(s['kind'],dict) and 'Call' in s['kind']);assigns=[s['kind']['Assign'] for s in ss if isinstance(s['kind'],dict) and 'Assign' in s['kind']]
  if label=='repr':cell['layout'][0]['value']['repr']['transparent']=False
  elif label=='field':cell['kind']['Struct'][0]['name']='other'
  elif label=='callee':call['call']['func']['Regular']['kind']['Fun']=f['def_id']
  elif label=='argument':call['call']['args'][0]['Move']['kind']={'Global':{}}
  elif label=='projection':assigns[0][1]['Ref']['place']['kind']['Projection'][1]['Field'][1]=1
  elif label=='metadata':assigns[0][1]['Ref']['ptr_metadata']['Const']={'Value':[99999,[{'Integer':{'Unsigned':['Usize','1']}},{'Value':[99999,{'TypeVar':{'Free':0}}]}]]}
  elif label=='retag':assigns[1][1]['Use'][1]='No'
  elif label=='unwind':call['on_unwind']['statements'][-1]['kind']='UnwindTerminate'
  else:ss[-1]['kind']='UnwindResume'
  try:lower(bad)
  except ValueError:pass
  else:raise AssertionError('Altered Cell get witness accepted: '+label)

if __name__=='__main__':
 ap=argparse.ArgumentParser();ap.add_argument('llbc');ap.add_argument('--output',required=True);ap.add_argument('--check',action='store_true');a=ap.parse_args();data=json.loads(pathlib.Path(a.llbc).read_text());text=render(lower(data));negative_controls(data);out=pathlib.Path(a.output)
 if a.check:
  if out.read_text()!=text:raise SystemExit('Actual Cell get IR drift')
 else:out.write_text(text)
 print('Actual Cell get main ten statements/callee/unwind/layout inspected; nine copied witness modifications rejected.')
