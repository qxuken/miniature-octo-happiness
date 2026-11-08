#!/usr/bin/env python3
"""Run bounded real ENOSPC and permission drills inside a fresh Linux container."""
import argparse
from pathlib import Path
import subprocess
import uuid

ROOT = Path(__file__).resolve().parent.parent

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--image', default='rust:1-bookworm')
    args = parser.parse_args()
    script = r'''
set -eu
uname -a
rustc --version
findmnt -n -o FSTYPE,SIZE,OPTIONS /faults
test "$(findmnt -n -o FSTYPE /faults)" = tmpfs
mkdir /work
tar -C /source --exclude=.git --exclude=target --exclude=node_modules -cf - . | tar -C /work -xf -
cd /work
CARGO_TARGET_DIR=/build cargo build --locked --example native_faults
mkdir /faults/db /faults/denied
chmod 700 /faults/denied
setpriv --reuid=10001 --regid=10001 --clear-groups /build/debug/examples/native_faults permission /faults/denied
test ! -e /faults/denied/database.wdb
WEEKEND_PRIVATE_TMPFS=1 /build/debug/examples/native_faults diskfull /faults/db
'''
    # Source is read-only. All writes and ballast are container-private, with
    # ballast confined to an 8 MiB tmpfs; no host filesystem is filled.
    name = 'weekend-native-faults-' + uuid.uuid4().hex
    try:
        subprocess.run(['docker', 'run', '--rm', '--name', name,
                        '--network=bridge', '--memory=2g', '--pids-limit=256',
                        '--tmpfs', '/faults:rw,size=8m,mode=755',
                        '--mount', f'type=bind,source={ROOT},target=/source,readonly',
                        args.image, 'sh', '-c', script], check=True, timeout=300)
    finally:
        subprocess.run(['docker', 'rm', '-f', name], stdout=subprocess.DEVNULL,
                       stderr=subprocess.DEVNULL, timeout=30, check=False)

if __name__ == '__main__':
    main()
