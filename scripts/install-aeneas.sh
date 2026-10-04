#!/usr/bin/env bash
# Install the pinned proof-only translator and Lean backend in ignored local storage.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"
python3 - "${repo_root}" <<'PYTHON'
import hashlib, json, pathlib, platform, subprocess, tarfile, tempfile
root = pathlib.Path(__import__('sys').argv[1])
lock = json.loads((root / 'proofs/extraction/toolchain.json').read_text())
key = platform.system() + '-' + platform.machine()
if key not in lock['platforms']:
    raise SystemExit('Unsupported pinned Aeneas platform: ' + key)
dest = root / '.local/proof-tools/aeneas'
if (dest / '.installed-release').exists():
    if (dest / '.installed-release').read_text().strip() != lock['aeneas']:
        raise SystemExit('Existing Aeneas installation does not match lock')
else:
    if dest.exists():
        raise SystemExit('Existing unmanaged Aeneas directory; inspect it before installing')
    dest.mkdir(parents=True)
    with tempfile.TemporaryDirectory(prefix='kasane-aeneas-') as temporary:
        for kind, asset in lock['platforms'][key].items():
            archive = pathlib.Path(temporary) / (kind + '.tar.gz')
            subprocess.run(['curl', '-fL', '--retry', '2', asset['url'], '-o', str(archive)], check=True)
            if hashlib.sha256(archive.read_bytes()).hexdigest() != asset['sha256']:
                raise SystemExit('Aeneas archive checksum mismatch: ' + kind)
            target = dest if kind == 'tools' else dest / 'backends/lean/.lake/build'
            target.mkdir(parents=True, exist_ok=True)
            with tarfile.open(archive) as package:
                for member in package.getmembers():
                    if member.name.startswith('/') or '..' in pathlib.PurePosixPath(member.name).parts:
                        raise SystemExit('Unsafe archive member: ' + member.name)
                    if member.issym() or member.islnk():
                        raise SystemExit('Unexpected archive link: ' + member.name)
                package.extractall(target)
    (dest / '.installed-release').write_text(lock['aeneas'] + '\n')
subprocess.run(['rustup', 'toolchain', 'install', lock['rust'], '--profile', 'minimal'], check=True)
subprocess.run(['elan', 'toolchain', 'install', lock['lean']], check=True)
subprocess.run(['lake', 'update'], cwd=dest / 'backends/lean', check=True)
subprocess.run(['lake', 'update'], cwd=root / 'proofs/extraction/lean', check=True)
PYTHON
