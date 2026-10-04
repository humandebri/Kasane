#!/usr/bin/env python3
"""Compare fixed compiler MIR witnesses; this is not a MIR semantics proof."""
from pathlib import Path
import hashlib
import json


def functions(text: str) -> dict[str, str]:
    result = {}
    lines = text.splitlines(True)
    i = 0
    while i < len(lines):
        if lines[i].startswith('fn '):
            name = lines[i].split('(', 1)[0][3:]
            start = i
            depth = 0
            while i < len(lines):
                depth += lines[i].count('{') - lines[i].count('}')
                i += 1
                if depth == 0:
                    break
            if depth or name in result:
                raise ValueError('Malformed or duplicate function witness')
            result[name] = ''.join(lines[start:i])
        else:
            i += 1
    return result


def audit(release: str, nightly: str) -> dict:
    r, n = functions(release), functions(nightly)
    names = {'duration_nanos', 'checked_duration_nanos', 'add_duration', 'sub_duration'}
    if set(r) != names or set(n) != names:
        raise ValueError('Unexpected MIR function inventory')
    if r['duration_nanos'] != n['duration_nanos']:
        raise ValueError('Duration::as_nanos body differs')
    for label, fs in [('release', r), ('nightly', n)]:
        for name, body in fs.items():
            if body.count('const 1000000000_u128') != 1 or ' = Mul(' not in body or ' = Add(' not in body:
                raise ValueError(label + ': nanos arithmetic shape differs')
            if name != 'duration_nanos' and body.count('const 18446744073709551615_u128') != 1:
                raise ValueError(label + ': checked conversion bound differs')
        if 'Gt(copy _2, const 18446744073709551615_u128)' not in fs['checked_duration_nanos']:
            raise ValueError(label + ': checked conversion comparison differs')
    if 'Err(TryFromIntError(()))' not in r['checked_duration_nanos']:
        raise ValueError('Release error payload changed')
    if 'Err(const TryFromIntError(PosOverflow))' not in n['checked_duration_nanos']:
        raise ValueError('Nightly error payload changed')
    if 'result::unwrap_failed' not in r['add_duration'] or 'result::unwrap_failed' not in r['sub_duration']:
        raise ValueError('Release unwrap control-flow changed')
    for fs in [r, n]:
        if 'std::intrinsics::saturating_add::<u64>' not in fs['add_duration'] or 'std::intrinsics::saturating_sub::<u64>' not in fs['sub_duration']:
            raise ValueError('Saturation operation changed')
    return {'duration_nanos_exact_MIR_body_equal': True,
            'common_body_sha256': hashlib.sha256(r['duration_nanos'].encode()).hexdigest(),
            'error_payload_equal': False, 'whole_MIR_equal': release == nightly,
            'semantic_equivalence_proved': False}


def main():
    root = Path(__file__).resolve().parent.parent
    cache = root / '.local/proof-tools/ledger-repro-cache'
    release = (cache / 'release-duration-wasm32.mir').read_text()
    nightly = (cache / 'nightly-duration-host.mir').read_text()
    print(json.dumps(audit(release, nightly), indent=2))
    mutations = [release.replace('1000000000_u128', '1000000001_u128'),
                 release.replace('18446744073709551615_u128', '18446744073709551614_u128'),
                 release.replace('Gt(copy _2', 'Ge(copy _2'),
                 release.replace('saturating_add::<u64>', 'saturating_sub::<u64>')]
    for mutation in mutations:
        try:
            audit(mutation, nightly)
        except ValueError:
            pass
        else:
            raise AssertionError('Changed arithmetic/bound/branch/saturation accepted')
    print('Four arithmetic/bound/branch/saturation mutations rejected.')


if __name__ == '__main__':
    main()
