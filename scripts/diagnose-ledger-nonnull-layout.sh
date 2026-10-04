#!/usr/bin/env bash
# Source-layout diagnostic and finite Rust/Miri probes; no pointer refinement theorem.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
export RUSTUP_TOOLCHAIN=nightly-2026-09-17
export CARGO_TARGET_DIR="$PWD/.local/proof-tools/ledger-timestamp-target"
export RUSTFLAGS='-Coverflow-checks=yes'
shasum -a 256 -c proofs/extraction/ledger-time-sources.sha256
python3 - <<'PY'
import copy,hashlib,json,os,pathlib,subprocess,tempfile
profile=json.load(open('proofs/extraction/tool-patches/nonnull-layout-profile.json'))
fixed=json.load(open('proofs/extraction/tool-patches/build-profile.json'))
def hashes():
 for mapping in [profile['sha256'],fixed['experimental_binary_sha256']]:
  for f,h in mapping.items():assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
hashes()
# Reuse the exact fixed source profile command; extend only its include list.
s=pathlib.Path('scripts/diagnose-ledger-fn-item-value.sh').read_text()
ns={};exec(s[s.index("cmd=['target/debug/charon'"):s.index("with tempfile.TemporaryDirectory(prefix='kasane-formatting-source-')")],ns)
cmd=ns['cmd']
include='core::ptr::non_null::NonNull'
def walk(x):
 yield x
 if isinstance(x,dict):
  for v in x.values():yield from walk(v)
 elif isinstance(x,list):
  for v in x:yield from walk(v)
def name(d):return [p['Ident'][0] for p in d['item_meta']['name'] if 'Ident'in p]
with tempfile.TemporaryDirectory(prefix='nonnull-source-layout-') as tmp:
 tmp=pathlib.Path(tmp)
 def extract(label,extra):
  dest=tmp/(label+'.llbc')
  subprocess.run(cmd+extra+['--opaque','core::panicking::panic_fmt','--dest-file',str(dest),'--','--manifest-path','proofs/extraction/ledger-time-rust/Cargo.toml','--lib','--locked'],check=True)
  data=json.loads(dest.read_text());assert not data['has_errors']
  assert all(not isinstance(v,dict) or ('Error'not in v and not v.get('has_errors',False)) for v in walk(data['translated']))
  assert sum(bool(d and isinstance(d['body'],dict) and 'Structured'in d['body']) for d in data['translated']['fun_decls'])==17
  return data,dest
 baseline,_=extract('baseline',[])
 data,dest=extract('nonnull',['--include',include])
 options=copy.deepcopy(baseline['translated']['options']);options['include'].append(include);options['dest_file']=str(dest)
 assert data['translated']['options']==options
 for key in ['skip_borrowck','no_typecheck','no_normalize','erase_body_lifetimes','reconstruct_panic_calls','no_compute_layout_guarantees']:
  assert options[key] is False,key
 nn=lambda x:next(d for d in x['translated']['type_decls'] if d and name(d)==['core','ptr','non_null','NonNull'])
 old=nn(baseline);assert old['kind']=='Opaque' and old['layout']==[]
 decl=nn(data)
 # Charon caches types, expressions and layout guarantees independently.
 definitions={}
 for node in walk(data):
  if isinstance(node,dict) and 'Value'in node and isinstance(node['Value'],list) and len(node['Value'])==2:
   definitions.setdefault(node['Value'][0],[]).append(node['Value'][1])
 def resolve(node,pred):
  if 'Value'in node:value=node['Value'][1]
  elif 'Untagged'in node:value=node['Untagged']
  else:
   candidates=[v for v in definitions[node['Deduplicated']] if pred(v)]
   assert len(candidates)==1,candidates;value=candidates[0]
  assert pred(value),value
  return value
 ty=lambda node:resolve(node,lambda v:isinstance(v,dict) and bool(set(v)&{'Pattern','RawPtr','TypeVar'}))
 guarantee=lambda node:resolve(node,lambda v:isinstance(v,dict) and set(v)=={'Constant'})
 def constant(node):return resolve(node,lambda v:isinstance(v,list) and len(v)==2 and isinstance(v[0],dict) and bool(set(v[0])&{'SizeOf','AlignOf'}))
 def supported(d,diagnose=False):
  try:
   assert name(d)==['core','ptr','non_null','NonNull'] and d['item_meta']['lang_item']=='NonNull'
   assert d['ptr_metadata']=='None'
   g=d['generics'];assert g['regions']==[] and len(g['types'])==1 and g['types'][0]['index']==0
   assert all(g[k]==[] for k in ['const_generics','trait_clauses','regions_outlive','types_outlive','trait_type_constraints'])
   fields=d['kind']['Struct'];assert len(fields)==1 and fields[0]['name']=='pointer'
   field=fields[0]['ty'];pattern=ty(field)['Pattern'];assert pattern[1]=='NotNull'
   pointer=ty(pattern[0])['RawPtr'];assert pointer[1]=='Shared' and ty(pointer[0])=={'TypeVar':{'Free':0}}
   assert len(d['layout'])==1
   layout=d['layout'][0];assert layout['key']=='aarch64-apple-darwin';l=layout['value']
   assert l['repr']=={'repr_algo':'Rust','align_modif':None,'transparent':True,'explicit_discr_type':None}
   assert l['size']['chosen'] is None and l['align']['chosen'] is None
   for k,op in [('size','SizeOf'),('align','AlignOf')]:
    expr=constant(guarantee(l[k]['guarantee'])['Constant']);assert set(expr[0])=={op} and ty(expr[0][op])==ty(field)
   assert len(l['variant_layouts'])==1 and l['variant_layouts'][0]['tagger']==[]
   offsets=l['variant_layouts'][0]['field_offsets'];assert len(offsets)==1 and offsets[0]['chosen'] is None
   assert guarantee(offsets[0]['guarantee']['GuaranteedAlignment'])==guarantee(l['align']['guarantee'])
   return True
  except (AssertionError,KeyError,TypeError):
   if diagnose:raise
   return False
 assert supported(decl,True)
 mutations=[]
 def bad(change):
  d=copy.deepcopy(decl);change(d);mutations.append(d)
 bad(lambda d:d.update(kind='Opaque'))
 bad(lambda d:d['kind']['Struct'].append(copy.deepcopy(d['kind']['Struct'][0])))
 bad(lambda d:d['kind']['Struct'][0].update(name='different'))
 bad(lambda d:d['generics']['types'].append(copy.deepcopy(d['generics']['types'][0])))
 bad(lambda d:d['generics']['types'][0].update(index=1))
 bad(lambda d:d.update(ptr_metadata='Slice'))
 bad(lambda d:d['layout'][0].update(key='other-target'))
 bad(lambda d:d['layout'][0]['value']['repr'].update(transparent=False))
 bad(lambda d:d['layout'][0]['value']['repr'].update(repr_algo='C'))
 bad(lambda d:d['layout'][0]['value']['size'].update(chosen=8))
 bad(lambda d:d['layout'][0]['value']['align'].update(chosen=8))
 bad(lambda d:d['layout'][0]['value']['variant_layouts'][0]['field_offsets'].append(copy.deepcopy(d['layout'][0]['value']['variant_layouts'][0]['field_offsets'][0])))
 assert len(mutations)==12 and all(not supported(d) for d in mutations)
 out=pathlib.Path('.local/proof-tools/ledger-nonnull-layout');out.mkdir(parents=True,exist_ok=True)
 (out/'LedgerNonNullLayoutSource.llbc').write_bytes(dest.read_bytes())
 print('17 clean bodies preserved; only NonNull include added; actual transparent single NotNull raw-pointer field and symbolic size/alignment guarantees recovered; 12 source-schema negatives rejected. No numerical generic layout or pointer semantic proof claimed.')
for tool in [['cargo'],['cargo','miri']]:
 subprocess.run(tool+['test','--manifest-path','proofs/extraction/ledger-time-rust/Cargo.toml','--locked','--test','array_nonnull_source'],check=True)
with tempfile.TemporaryDirectory(prefix='nonnull-empty-miri-') as neg:
 source=pathlib.Path('proofs/extraction/tool-patches/tests/empty_array_nonnull_fail.rs').resolve()
 manifest=pathlib.Path(neg)/'Cargo.toml'
 manifest.write_text('[package]\nname="nonnull-empty-negative"\nversion="0.0.0"\nedition="2021"\n[workspace]\n[lib]\npath='+json.dumps(str(source))+'\n')
 r=subprocess.run(['cargo','miri','test','--manifest-path',str(manifest)],capture_output=True,text=True,env={**os.environ,'CARGO_TARGET_DIR':str(pathlib.Path(neg)/'target')})
 pathlib.Path('/private/tmp/kasane-nonnull-empty-miri.log').write_text(r.stdout+r.stderr)
 assert r.returncode!=0 and 'Undefined Behavior'in r.stdout+r.stderr and 'empty_array_nonnull_fail.rs'in r.stdout+r.stderr and 'invalid value of type &u8'in r.stdout+r.stderr,r.stdout+r.stderr
 print('Miri-only empty-array element dereference rejected as invalid &u8; retaining pointer alone does not establish a readable element.')
hashes()
PY
