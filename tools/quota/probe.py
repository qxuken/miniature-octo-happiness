#!/usr/bin/env python3
"""Guest-side bounded quota calibration. Never configures a filesystem."""
import argparse
import json
import os
from pathlib import Path
import stat

LIMIT = 8 * 1024 * 1024

def identity(uid):
    fields = dict(line.split(':', 1) for line in Path('/proc/self/status').read_text().splitlines() if ':' in line)
    ids = {name: [int(n) for n in fields[name].split()] for name in ['Uid', 'Gid', 'Groups']}
    caps = {name: int(fields[name].strip(), 16) for name in ['CapInh', 'CapPrm', 'CapEff', 'CapBnd', 'CapAmb']}
    if ids['Uid'] != [uid]*4 or ids['Gid'] != [uid]*4 or ids['Groups'] or any(caps.values()) or fields['NoNewPrivs'].strip() != '1':
        raise RuntimeError('guest privilege boundary not established')
    return {'uid': uid, 'uids': ids['Uid'], 'gids': ids['Gid'], 'groups': [], 'caps': caps, 'no_new_privs': 1}

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=['identity', 'fill', 'remove', 'write-control'])
    parser.add_argument('directory', type=Path)
    parser.add_argument('uid', type=int)
    args = parser.parse_args()
    result = {'mode': args.mode, 'identity': identity(args.uid), 'pid': os.getpid()}
    if args.directory.is_symlink() or not args.directory.is_dir():
        raise RuntimeError('private data directory required')
    ballast = args.directory / 'ballast'
    if args.mode == 'fill':
        fd = os.open(ballast, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        written = 0; failure = None
        try:
            block = bytes([0x5a]) * 65536
            while written < LIMIT:
                try:
                    size = os.write(fd, block[:min(len(block), LIMIT-written)])
                    if size <= 0: raise RuntimeError('zero-progress ballast write')
                    written += size
                    os.fsync(fd)
                except OSError as error:
                    failure = error.errno
                    break
        finally:
            os.close(fd)
        result.update(errno=failure, written=written, ceiling=LIMIT)
    elif args.mode == 'remove':
        info = ballast.lstat()
        if not stat.S_ISREG(info.st_mode) or info.st_uid != args.uid:
            raise RuntimeError('ballast ownership/type mismatch')
        ballast.unlink()
        directory_fd=os.open(args.directory,os.O_RDONLY|os.O_DIRECTORY)
        try:os.fsync(directory_fd)
        finally:os.close(directory_fd)
        result['removed'] = True
    elif args.mode == 'write-control':
        path = args.directory / 'calibration-control'
        fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        block = bytes([0xa7]) * 4096
        try:
            if os.write(fd, block) != len(block): raise RuntimeError('short control write')
            os.fsync(fd)
        finally:
            os.close(fd)
        if path.read_bytes() != block: raise RuntimeError('control contents changed')
        path.unlink(); result['fsynced_bytes'] = len(block)
    space = os.statvfs(args.directory)
    result['global_free_bytes'] = space.f_bfree * space.f_frsize
    result['available_bytes'] = space.f_bavail * space.f_frsize
    print(json.dumps(result, separators=(',', ':')))

if __name__ == '__main__': main()
