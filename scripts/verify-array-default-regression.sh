#!/usr/bin/env bash
# Regression for unused metadata handling, not proof of EVM instruction semantics.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
shasum -a 256 -c proofs/extraction/array-default-sources.sha256
export DYLD_LIBRARY_PATH="$PWD/.local/proof-tools/aeneas/libs${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
python3 - <<'PYTEST'
import hashlib,json,pathlib,subprocess,tempfile
profile=json.load(open('proofs/extraction/tool-patches/build-profile.json'))
exe='.local/proof-tools/aeneas-source/src/_build/default/main.exe'
assert hashlib.sha256(pathlib.Path(exe).read_bytes()).hexdigest()==profile['experimental_binary_sha256'][exe]
fixture=pathlib.Path('proofs/extraction/tool-patches/fixtures/revm-concrete-mono.json')
with tempfile.TemporaryDirectory(prefix='kasane-array-default-') as tmp:
    for case in ['unreferenced','referenced']:
        data=json.loads(fixture.read_text())
        if case=='referenced':
            data['translated']['ordered_decls'].append({'TraitImpl':{'NonRec':0}})
        src=pathlib.Path(tmp)/f'{case}.llbc'
        src.write_text(json.dumps(data))
        result=subprocess.run([exe,'-backend','lean','-dest',tmp,'-namespace','RevmConcrete','-use-lean-modules','false','-abort-on-error','-warnings-as-errors',str(src)],capture_output=True,text=True)
        output=result.stdout+result.stderr
        pathlib.Path(f'/private/tmp/kasane-array-default-{case}.log').write_text(output)
        assert result.returncode!=0, 'EVM translation is still expected to fail downstream'
        if case=='unreferenced':
            assert 'Applied prepasses:' in output and 'llbc/TypesAnalysis.ml' in output, output
            assert 'Compiler source: PrePasses.ml' not in output, output
            print('unreferenced metadata: prepasses passed; actual translation still stops at erased-region type analysis')
        else:
            assert 'Compiler source: PrePasses.ml' in output, output
            print('referenced invalid metadata: strict rejection preserved')
PYTEST
