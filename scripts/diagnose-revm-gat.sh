#!/usr/bin/env bash
# Reproduces a known frontend blocker; success is NOT an instruction proof.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
shasum -a 256 -c proofs/extraction/gat-diagnostic-sources.sha256
export RUSTUP_TOOLCHAIN=nightly-2026-09-17
export DYLD_LIBRARY_PATH="$PWD/.local/proof-tools/aeneas/libs${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
export CARGO_TARGET_DIR="$PWD/.local/proof-tools/gat-diagnostic-target"
python3 - <<'PYTEST'
import hashlib,json,pathlib,subprocess,tempfile
profile=json.load(open('proofs/extraction/tool-patches/build-profile.json'))
for filename,expected in profile['experimental_binary_sha256'].items():
    assert hashlib.sha256(pathlib.Path(filename).read_bytes()).hexdigest()==expected,filename
manifest='proofs/extraction/gat-diagnostic-rust/Cargo.toml'
with tempfile.TemporaryDirectory(prefix='kasane-gat-diagnostic-') as tmp:
    for case in ['plain','rpit','named']:
        args=['--manifest-path',manifest,'--lib','--features',case,'--locked']
        check=subprocess.run(['cargo','check',*args],capture_output=True,text=True)
        assert check.returncode==0,check.stdout+check.stderr
        dest=pathlib.Path(tmp)/f'kasane-gat-{case}.llbc'
        command=['target/debug/charon','cargo','--preset=aeneas','--mir','promoted',
          '--error-on-warnings','--dest-file',str(dest),'--',*args]
        result=subprocess.run(command,capture_output=True,text=True)
        output=result.stdout+result.stderr
        if case=='plain':
            assert result.returncode==0,output
            data=json.loads(dest.read_text())
            assert data['has_errors'] is False,data['has_errors']
            backend=subprocess.run(['.local/proof-tools/aeneas-source/src/_build/default/main.exe',
              '-backend','lean','-dest',tmp,'-namespace','GatPlain','-use-lean-modules','false',
              '-abort-on-error','-warnings-as-errors',str(dest)],capture_output=True,text=True)
            assert backend.returncode==0,backend.stdout+backend.stderr
            generated=pathlib.Path(tmp)/'Kasane-gat-plain.lean'
            assert generated.exists()
            assert not any(line.lstrip().startswith('axiom ') for line in generated.read_text().splitlines())
            print('plain reference return: Rust check and clean Charon/Aeneas extraction passed (control only)')
        else:
            assert result.returncode!=0, 'Known GAT blocker changed: review new extraction before accepting it'
            assert 'GATs cannot work with the `--lift-associated-types` option' in output,output
            assert dest.exists(), 'Expected fixed Charon to emit an explicitly invalid diagnostic LLBC'
            failed=json.loads(dest.read_text())
            assert failed['has_errors'] is True, 'Failed GAT extraction must be marked invalid'
            invalid=[item for group in ['trait_decls','trait_impls']
              for item in failed['translated'][group] if item and item['item_meta']['has_errors']]
            assert invalid, 'Expected GAT error metadata on a trait or impl'
            dest.unlink()  # Do not pass an invalid diagnostic LLBC to Aeneas.
            print(f'{case}: valid Rust; Charon preset=aeneas rejects GAT lifting as expected (still unproved)')
print('[diagnose-revm-gat] known frontend limitation reproduced; GAT/RPITIT and EVM instruction correspondence remain unproved')
PYTEST
