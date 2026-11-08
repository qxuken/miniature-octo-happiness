#!/usr/bin/env python3
"""Build and verify a private native package, with provenance and no publication."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import release_notices

ROOT = Path(__file__).resolve().parent.parent


def capture(*command):
    return subprocess.check_output(command, cwd=ROOT, text=True).strip()


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def build_configuration(env):
    """Hash overrides/configs, never copy their potentially secret contents."""
    names = {'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC', 'RUSTC_WRAPPER',
             'RUSTC_WORKSPACE_WRAPPER', 'RUSTUP_TOOLCHAIN', 'CARGO_HOME',
             'CC', 'CXX', 'CFLAGS', 'CXXFLAGS', 'LDFLAGS'}
    overrides = {name: hashlib.sha256(value.encode()).hexdigest()
                 for name, value in env.items()
                 if name in names or name.startswith(('CARGO_PROFILE_', 'CARGO_BUILD_', 'CARGO_TARGET_'))}
    locations = [directory / '.cargo' for directory in [ROOT, *ROOT.parents]]
    cargo_home = Path(env.get('CARGO_HOME', str(Path.home() / '.cargo'))).expanduser()
    locations.append(cargo_home if cargo_home.is_absolute() else ROOT / cargo_home)
    configs = {}
    for directory in locations:
        # Cargo prefers legacy config when both filenames exist.
        path = directory / 'config'
        if not path.is_file():
            path = directory / 'config.toml'
        if path.is_file():
            configs[str(path.resolve())] = digest(path)
    return {'override_sha256': overrides, 'cargo_config_sha256': configs}


def require_unwind(output):
    strategies = re.findall(r'^panic="([^"]+)"$', output, re.MULTILINE)
    if strategies != ['unwind']:
        raise RuntimeError('The delivered library must use panic=unwind. Remove conflicting Rust flags or Cargo configuration.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True, help='New package directory')
    parser.add_argument('--features', default='')
    parser.add_argument('--allow-dirty', action='store_true', help='Mark package ineligible as a release candidate')
    args = parser.parse_args()
    dirty = capture('git', 'status', '--porcelain')
    if dirty and not args.allow_dirty:
        parser.error('A release candidate requires a clean source tree. --allow-dirty creates an ineligible development package.')
    if sys.platform not in ('darwin', 'linux'):
        parser.error('Native package qualification currently supports macOS and Linux. Windows compiled C/C++ qualification remains pending.')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    source = capture('git', 'rev-parse', 'HEAD')
    source_id = source + ('-dirty' if dirty else '')
    def source_hashes():
        names = capture('git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z').split('\0')
        return {name: digest(ROOT / name) for name in names
                if name and (ROOT / name).is_file() and not (ROOT / name).resolve().is_relative_to(output)}

    initial_hashes = source_hashes()
    env = os.environ.copy()
    inherited_configuration = build_configuration(env)
    env['WEEKEND_DB_SOURCE_ID'] = source_id
    env['CARGO_PROFILE_RELEASE_PANIC'] = 'unwind'
    # Keep candidate products isolated from ordinary developer builds.
    build = Path(tempfile.mkdtemp(prefix='weekend-release-'))
    evidence = {'source': source, 'source_id': source_id, 'dirty_status': dirty,
                'candidate_eligible': False, 'verified': False,
                'features': args.features, 'toolchain': capture('rustc', '-Vv'),
                'cargo': capture('cargo', '-V'), 'lockfile_sha256': digest(ROOT / 'Cargo.lock'),
                'source_files_sha256': initial_hashes, 'commands': [],
                'inherited_build_configuration': inherited_configuration,
                'effective_build_configuration': build_configuration(env)}

    def run(*command, collect=False):
        command = [str(item) for item in command]
        evidence['commands'].append(command)
        with (output / 'checks.log').open('a') as log:
            if collect:
                result = subprocess.run(command, cwd=ROOT, env=env, stdout=subprocess.PIPE,
                                        stderr=subprocess.STDOUT, text=True)
                log.write(result.stdout)
                result.check_returncode()
                return result.stdout
            subprocess.run(command, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)

    try:
        command = ['cargo', 'rustc', '--locked', '--release', '-p', 'weekend-db', '--lib', '--target-dir', build]
        if args.features:
            command.extend(['--features', args.features])
        command.extend(['--', '-C', 'panic=unwind'])
        # RUSTFLAGS can appear after cargo-rustc arguments. Ask the actual
        # compiler which panic strategy the exact crate configuration selects.
        query = [str(item) for item in command] + ['--print=cfg']
        evidence['commands'].append(query)
        with (output / 'checks.log').open('a') as log:
            cfg = subprocess.check_output(query, cwd=ROOT, env=env, text=True, stderr=log)
        require_unwind(cfg)
        evidence['panic_strategy'] = 'unwind'
        command.insert(command.index('--'), '--message-format=json-render-diagnostics')
        built = run(*command, collect=True)
        package_ids = set()
        for line in built.splitlines():
            if line.startswith('{'):
                message = json.loads(line)
                if message.get('reason') == 'compiler-artifact':
                    package_ids.add(message['package_id'])
        metadata_command = ['cargo', 'metadata', '--offline', '--locked', '--format-version=1']
        if args.features:
            metadata_command.extend(['--features', args.features])
        metadata = next(json.loads(line) for line in run(*metadata_command, collect=True).splitlines()
                        if line.startswith('{'))
        attribution = release_notices.collect(metadata, package_ids, ROOT / 'Cargo.lock', output / 'notices',
                                             Path(capture('rustc', '--print', 'sysroot')), evidence['toolchain'])
        evidence['attribution_inputs_complete'] = attribution['attribution_inputs_complete']
        evidence['attribution_review_items'] = attribution['review_items']
        filename = 'libweekend_db.dylib' if sys.platform == 'darwin' else 'libweekend_db.so'
        library = output / filename
        shutil.copy2(build / 'release' / filename, library)
        for name in ['LICENSE-MIT', 'LICENSE-APACHE', 'Cargo.lock']:
            shutil.copy2(ROOT / name, output / name)
        shutil.copy2(ROOT / 'include/weekend_db.h', output / 'weekend_db.h')
        header = (output / 'weekend_db.h').read_text()
        declarations = set(re.findall(r'\b(wdb_\w+)\s*\(', header))
        nm_command = ['nm', '-gU', str(library)] if sys.platform == 'darwin' else ['nm', '-D', '--defined-only', str(library)]
        symbols = capture(*nm_command)
        (output / 'symbols.txt').write_text(symbols + '\n')
        exports = set(re.findall(r'\b_?(wdb_\w+)\b', symbols))
        if declarations != exports:
            raise RuntimeError(f'Header/export mismatch, missing {declarations - exports}, extra {exports - declarations}')
        for compiler, language, standard, name in [
            (os.environ.get('CC', 'cc'), 'c', 'c11', 'consumer-c'),
            (os.environ.get('CXX', 'c++'), 'c++', 'c++17', 'consumer-cpp'),
        ]:
            evidence[name + '_compiler'] = capture(compiler, '--version')
            executable = output / name
            run(compiler, '-x', language, f'-std={standard}', '-Wall', '-Wextra', '-Werror',
                '-I', output, ROOT / 'examples/memory.c', '-x', 'none', library,
                f'-Wl,-rpath,{output}', '-o', executable)
            run(executable)
        run(sys.executable, ROOT / 'tools/ffi_smoke.py', library)
        # Assert embedded build identity independently of the generic ABI smoke.
        import ctypes
        api = ctypes.CDLL(str(library))
        api.wdb_source_id.argtypes = [ctypes.c_void_p, ctypes.c_size_t, ctypes.POINTER(ctypes.c_size_t)]
        api.wdb_source_id.restype = ctypes.c_int32
        required = ctypes.c_size_t()
        buffer = ctypes.create_string_buffer(256)
        assert api.wdb_source_id(buffer, len(buffer), ctypes.byref(required)) == 0
        assert buffer.raw[:required.value].decode() == source_id
        if (source_hashes() != initial_hashes or capture('git', 'rev-parse', 'HEAD') != source
                or build_configuration(env) != evidence['effective_build_configuration']):
            raise RuntimeError('Source changed during packaging. Rebuild from a stable source tree.')
        evidence['verified'] = True
        evidence['candidate_eligible'] = not bool(dirty) and attribution['attribution_inputs_complete']
    finally:
        evidence['sha256'] = {str(path.relative_to(output)): digest(path)
                              for path in output.rglob('*') if path.is_file() and path.name != 'manifest.json'}
        (output / 'manifest.json').write_text(json.dumps(evidence, indent=2) + '\n')
        shutil.rmtree(build)
    print(f'Verified private package {output}, candidate eligible {evidence["candidate_eligible"]}')


if __name__ == '__main__':
    main()
