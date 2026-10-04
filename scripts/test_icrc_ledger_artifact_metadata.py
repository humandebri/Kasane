#!/usr/bin/env python3
"""Reject malformed identity metadata; no instruction/semantics validation claim."""
from pathlib import Path
import json
from audit_icrc_ledger_artifact import audit_metadata, custom_sections

root = Path(__file__).resolve().parent.parent
profile = json.loads((root / 'proofs/external/ledger-profile.json').read_text())
data = (root / profile['bundled_wasm']).read_bytes()
sections = custom_sections(data)
commit = profile['source_commit']
candid = sections['icp:public candid:service'][0]
audit_metadata(data, commit, candid)
for name, wrong_commit, wrong_candid in [('commit', '0' * 40, candid), ('candid', commit, candid + b'x')]:
    try:
        audit_metadata(data, wrong_commit, wrong_candid)
    except ValueError:
        pass
    else:
        raise AssertionError(name + ' mismatch accepted')


def u32(n):
    out = bytearray()
    while n >= 128:
        out.append((n & 127) | 128)
        n >>= 7
    return bytes(out + bytes([n]))


def custom(name, payload):
    name = name.encode()
    contents = u32(len(name)) + name + payload
    return b'\0' + u32(len(contents)) + contents


for name, broken in [
    ('header', b'broken' + data[6:]),
    ('truncated', data[:-1]),
    ('oversized length', b'\0asm\1\0\0\0\0\xff\xff\xff\xff\x7f'),
    ('duplicate commit', data + custom('icp:public git_commit_id', (commit + '\n').encode())),
    ('duplicate Candid', data + custom('icp:public candid:service', candid)),
]:
    try:
        audit_metadata(broken, commit, candid)
    except ValueError:
        pass
    else:
        raise AssertionError(name + ' accepted')
print('Fixed Wasm identity metadata accepted; seven wrong/malformed/duplicate metadata controls rejected.')

# Byte comparison must reject a changed artifact even if its identity metadata stays intact.
import subprocess
import sys
import tempfile
with tempfile.TemporaryDirectory(prefix='ledger-artifact-negative-') as tmp:
    changed = data + custom('extra-artifact-marker', b'changed')
    audit_metadata(changed, commit, candid)
    wrong = Path(tmp) / 'changed.wasm'
    wrong.write_bytes(changed)
    result = subprocess.run([sys.executable, str(root / 'scripts/audit_icrc_ledger_artifact.py'),
                             '--rebuilt-wasm', str(wrong)], capture_output=True, text=True)
    assert result.returncode != 0 and 'rebuilt artifact bytes differ' in result.stderr, result.stderr
print('Changed artifact rejected by full-byte comparison; metadata equality is not enough.')
