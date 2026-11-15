"""Shared helpers for the remaining legacy POSIX runners."""
import hashlib
import os
from pathlib import Path
import signal
import sys

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "tools"))


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_hashes(source):
    return {str(p.relative_to(source)): digest(p) for p in sorted(source.rglob('*')) if p.is_file()}


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


def stop(child):
    # Every caller owns a child started with start_new_session=True. Descendants
    # can outlive an already-exited leader, so its status must not gate killpg.
    try:
        os.killpg(child.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    child.wait(timeout=5)


def run(command, output, evidence, label, *, cwd=ROOT, env=None, timeout=30):
    import supervision
    command = [str(arg) for arg in command]
    log_path = output / (label + '.log')
    record = {'command': command, 'cwd': str(cwd), 'timeout_seconds': timeout,
              'log': str(log_path), 'returncode': None}
    evidence['commands'].append(record)
    result = supervision.request(command, log_path, log_path, cwd=cwd, env=env, timeout=timeout)
    record.update(result)
    try: supervision.require_success(result, command, timeout)
    except ValueError as error: raise RuntimeError(str(error)) from error
    return log_path.read_text()
