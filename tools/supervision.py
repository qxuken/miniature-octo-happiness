"""Thin JSON transport to the private Rust supervisor; no child ownership loop here.

Build once with `cargo xtask --help`, or set WEEKEND_XTASK_BIN to a native
prebuilt runner. Requests/environments are sent over stdin, never saved as proof.
"""
import json
import hashlib
import os
from pathlib import Path
import signal
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
RAW_POPEN = subprocess.Popen

def binary():
    candidate = Path(os.environ.get('WEEKEND_XTASK_BIN', ROOT/'target/xtask/debug'/('xtask.exe' if os.name=='nt' else 'xtask')))
    if not candidate.is_file():
        raise RuntimeError('build the private runner with cargo xtask --help, or set WEEKEND_XTASK_BIN')
    return candidate.resolve()

def request(argv, stdout, stderr, *, cwd=None, env=None, timeout=30, maximum=8*1024*1024,
            exclusive=False, file_limit=None, journal=None, sample_rss=False, report=None):
    payload = dict(argv=list(map(str,argv)),cwd=str(cwd) if cwd else None,env=env,
                   stdout=str(Path(stdout).absolute()),stderr=str(Path(stderr).absolute()),
                   report=str(Path(report).absolute()) if report else None,timeout_seconds=timeout,
                   maximum_bytes=maximum,exclusive_limit=exclusive,file_limit=file_limit,
                   journal=str(Path(journal).absolute()) if journal else None,sample_rss=sample_rss)
    # Keep injection intended for a fixture out of the transport executable.
    transport_env = os.environ.copy()
    for key in ('LD_PRELOAD','LD_DEBUG','LD_LIBRARY_PATH'): transport_env.pop(key,None)
    child = RAW_POPEN([str(binary()),'supervise'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,
                     stderr=subprocess.PIPE,env=transport_env)
    try:
        out, err = child.communicate(json.dumps(payload).encode(),timeout=timeout+20)
    except KeyboardInterrupt:
        child.send_signal(signal.SIGTERM)
        try: out,err=child.communicate(timeout=12)
        except subprocess.TimeoutExpired:
            child.kill();child.communicate();raise RuntimeError('cancelled Rust supervisor did not retire')
        # Return the terminal status so the adapter can persist PID/exit/cleanup
        # before require_success re-raises cancellation.
    except BaseException:
        child.send_signal(signal.SIGTERM)
        try: child.communicate(timeout=12)
        except subprocess.TimeoutExpired: child.kill(); child.communicate()
        raise
    if child.returncode or len(out)>8*1024*1024:
        raise RuntimeError('Rust supervisor protocol failed: '+err.decode(errors='replace')[:4096])
    result = json.loads(out)
    result['supervisor'] = str(binary())
    result['supervisor_sha256'] = hashlib.sha256(binary().read_bytes()).hexdigest()
    return result

def require_success(result, argv, timeout, *, allowed=(0,)):
    if result['cleanup_failure'] or not result['cleanup_verified']:
        raise ValueError('process cleanup unverified: '+str(result['cleanup_failure']))
    if result['timed_out']: raise subprocess.TimeoutExpired(list(map(str,argv)),timeout)
    if result['cancelled']: raise KeyboardInterrupt('supervisor cancelled')
    if result['failure'] and result['returncode'] in allowed and result['failure'].startswith('child exit '):return
    if result['failure'] or result['returncode'] not in allowed:
        raise ValueError(result['failure'] or 'command failed')

def command(args):
    """Compatibility entry point for old Python CLIs; Rust owns orchestration."""
    raise SystemExit(subprocess.call([str(binary()),*map(str,args)]))

def docker_command(argv, image, output, build_root="/data"):
    """Stage today's tool separately from an archived engine, build it natively.

    The caller owns/copies/removes its Docker volume; the runner binary stays on
    that volume. All builds are offline against the pinned host registry cache.
    """
    seed=Path(output)/'supervisor-source'
    logs=Path(output)/'supervisor-seed'
    result=request([binary(),'runner-seed','--output',seed],logs.with_suffix('.stdout'),logs.with_suffix('.stderr'))
    require_success(result,['runner-seed'],30)
    registry=Path(os.environ.get('CARGO_HOME',Path.home()/'.cargo'))/'registry'
    index=argv.index(image)
    wrapper=['--mount',f'type=bind,source={seed.resolve()},target=/xtask-source,readonly',
             '--mount',f'type=bind,source={registry.resolve()},target=/usr/local/cargo/registry,readonly',
             '-e',f'WEEKEND_XTASK_BIN={build_root}/supervisor/debug/xtask',image,'sh','-c',
             f'set -eu; cargo build --offline --locked --manifest-path /xtask-source/Cargo.toml -p xtask --target-dir {build_root}/supervisor; exec "$@"',
             'xtask-worker']
    return [*argv[:index],*wrapper,*argv[index+1:]]

def identity():
    files=[ROOT/'Cargo.lock',ROOT/'xtask/Cargo.toml',*sorted((ROOT/'xtask/src').rglob('*')),*sorted((ROOT/'xtask/tests').rglob('*'))]
    return {'binary':str(binary()),'binary_sha256':hashlib.sha256(binary().read_bytes()).hexdigest(),
            'source_sha256':{str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for p in files if p.is_file()}}
