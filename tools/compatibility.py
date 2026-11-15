"""Shared helpers for the remaining legacy POSIX runners."""
import os
from pathlib import Path


def fresh_output(path):
    candidate = Path(os.path.abspath(path))
    if os.path.lexists(candidate):
        raise ValueError('output already exists, including dangling symlinks')
    candidate.parent.mkdir(parents=True, exist_ok=True)
    # Resolve parents only; a final-component symlink race must fail at mkdir.
    output = candidate.parent.resolve() / candidate.name
    output.mkdir(mode=0o700, exist_ok=False)
    return output


def child_env():
    env = os.environ.copy()
    for name in list(env):
        if (name in ('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'CARGO_BUILD_RUSTFLAGS',
                     'RUSTC', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER', 'CARGO_BUILD_TARGET')
                or name.startswith('CARGO_PROFILE_')
                or (name.startswith('CARGO_TARGET_') and name.endswith('_RUSTFLAGS'))):
            env.pop(name, None)
    env['CARGO_INCREMENTAL'] = '0'
    return env
