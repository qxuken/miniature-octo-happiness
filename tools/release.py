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
import windows_consumers
import odin_bindings

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


def require_dynamic_crt(cfg):
    if 'target_feature="crt-static"' in cfg.splitlines():
        raise RuntimeError('Windows packages require dynamic CRT; static MSVC runtime redistribution is outside this package contract.')


def candidate_eligible(dirty, attribution_complete):
    return not bool(dirty) and attribution_complete


def platform_artifacts(platform):
    if platform == 'win32':
        return ('weekend_db.dll', 'weekend_db.dll.lib')
    if platform == 'darwin':
        return ('libweekend_db.dylib',)
    if platform == 'linux':
        return ('libweekend_db.so',)
    raise RuntimeError('Native package qualification supports macOS, Linux and Windows MSVC')


def windows_tool(name, env):
    path = next((value for key, value in env.items() if key.lower() == 'path'), '')
    tool = shutil.which(name, path=path)
    if tool is None:
        raise RuntimeError(f'MSVC environment did not provide {name}')
    return tool


def windows_toolchain_identity(env):
    names = {'vctoolsversion', 'windowssdkversion', 'ucrtversion', 'vscmd_arg_tgt_arch'}
    identity = {key.lower(): value for key, value in env.items() if key.lower() in names}
    if not all(identity.get(key) for key in ['vctoolsversion', 'windowssdkversion']):
        raise RuntimeError('MSVC environment must identify VCToolsVersion and WindowsSDKVersion; run from a Visual Studio developer shell.')
    return identity


def symbol_command(platform, library, env):
    if platform == 'win32':
        return [windows_tool('dumpbin.exe', env), '/nologo', '/exports', str(library)]
    if platform == 'darwin':
        return ['nm', '-gU', str(library)]
    return ['nm', '-D', '--defined-only', str(library)]


def consumer_commands(platform, output, library, env):
    commands = []
    for language, standard, name in [('c', 'c11', 'consumer-c'), ('c++', 'c++17', 'consumer-cpp')]:
        if platform == 'win32':
            compiler = windows_tool('cl.exe', env)
            executable = output / (name + '.exe')
            command = [compiler, '/nologo', '/W4', '/WX', '/MD',
                       '/TC' if language == 'c' else '/TP', '/std:' + standard,
                       '/I' + str(output), str(ROOT / 'examples/memory.c'),
                       '/Fo' + str(output / (name + '.obj')), '/Fe' + str(executable),
                       '/link', str(output / 'weekend_db.dll.lib')]
            version = [compiler, '/?']
        else:
            compiler = env.get('CC' if language == 'c' else 'CXX', 'cc' if language == 'c' else 'c++')
            executable = output / name
            command = [compiler, '-x', language, '-std=' + standard, '-Wall', '-Wextra', '-Werror',
                       '-I', str(output), str(ROOT / 'examples/memory.c'), '-x', 'none', str(library),
                       '-Wl,-rpath,' + ('@loader_path' if platform == 'darwin' else '$ORIGIN'), '-o', str(executable)]
            version = [compiler, '--version']
        commands.append((name, version, command, executable))
    return commands


def library_link_flags(platform):
    if platform == 'darwin':
        return ['-C', 'link-arg=-Wl,-install_name,@rpath/libweekend_db.dylib']
    if platform == 'linux':
        return ['-C', 'link-arg=-Wl,-soname,libweekend_db.so']
    return []


def dependency_command(platform, artifact, env):
    if platform == 'win32':
        return [windows_tool('dumpbin.exe', env), '/nologo', '/dependents', str(artifact)]
    return ['otool', '-L', str(artifact)] if platform == 'darwin' else ['readelf', '-d', str(artifact)]


def find_odin(env):
    compiler = shutil.which('odin', path=next((value for key, value in env.items() if key.lower() == 'path'), ''))
    if compiler is None:
        raise RuntimeError('--odin requires an installed Odin compiler; qualification cannot be skipped')
    return str(Path(compiler).resolve())


def odin_command(platform, compiler, output, library):
    executable = output / ('consumer-odin.exe' if platform == 'win32' else 'consumer-odin')
    link = output / 'weekend_db.dll.lib' if platform == 'win32' else library
    relative = os.path.relpath(link, output / 'bindings/odin').replace('\\', '/')
    command = [compiler, 'build', str(output / 'odin-consumer'), '-out:' + str(executable),
               '-collection:weekend=' + str(output / 'bindings'), '-define:WEEKEND_DB_LIBRARY=' + relative]
    if platform != 'win32':
        command.append('-extra-linker-flags:-Wl,-rpath,' + ('@loader_path' if platform == 'darwin' else '$ORIGIN'))
    return command, executable


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True, help='New package directory')
    parser.add_argument('--features', default='')
    parser.add_argument('--odin', action='store_true', help='Package and require exact-artifact Odin qualification')
    parser.add_argument('--allow-dirty', action='store_true', help='Mark package ineligible as a release candidate')
    args = parser.parse_args()
    dirty = capture('git', 'status', '--porcelain')
    if dirty and not args.allow_dirty:
        parser.error('A release candidate requires a clean source tree. --allow-dirty creates an ineligible development package.')
    if sys.platform not in ('darwin', 'linux', 'win32'):
        parser.error('Native package qualification supports macOS, Linux and Windows MSVC.')
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

    log_output = output

    def run(*command, collect=False):
        command = [str(item) for item in command]
        evidence['commands'].append(command)
        with (log_output / 'checks.log').open('a') as log:
            if collect:
                result = subprocess.run(command, cwd=ROOT, env=env, stdout=subprocess.PIPE,
                                        stderr=subprocess.STDOUT, text=True)
                log.write(result.stdout)
                result.check_returncode()
                return result.stdout
            subprocess.run(command, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)

    try:
        if sys.platform == 'win32':
            if 'host: x86_64-pc-windows-msvc' not in evidence['toolchain'].splitlines():
                raise RuntimeError('Windows native packaging currently qualifies the x86_64 MSVC host only.')
            env = windows_consumers.compiler_environment(build)
            # VS setup inherits process variables; retain the packaging overrides.
            env['WEEKEND_DB_SOURCE_ID'] = source_id
            env['CARGO_PROFILE_RELEASE_PANIC'] = 'unwind'
            evidence['effective_build_configuration'] = build_configuration(env)
        # Metadata inventories the workspace, including dependencies not built
        # by this library profile. Populate that locked input set on fresh hosts.
        run('cargo', 'fetch', '--locked')
        command = ['cargo', 'rustc', '--locked', '--release', '-p', 'weekend-db', '--lib', '--target-dir', build]
        if args.features:
            command.extend(['--features', args.features])
        command.extend(['--', '-C', 'panic=unwind'])
        command.extend(library_link_flags(sys.platform))
        # RUSTFLAGS can appear after cargo-rustc arguments. Ask the actual
        # compiler which panic strategy the exact crate configuration selects.
        query = [str(item) for item in command] + ['--print=cfg']
        evidence['commands'].append(query)
        with (log_output / 'checks.log').open('a') as log:
            cfg = subprocess.check_output(query, cwd=ROOT, env=env, text=True, stderr=log)
        require_unwind(cfg)
        if sys.platform == 'win32':
            require_dynamic_crt(cfg)
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
        filename = platform_artifacts(sys.platform)[0]
        library = output / filename
        for artifact in platform_artifacts(sys.platform):
            shutil.copy2(build / 'release' / artifact, output / artifact)
        for name in ['LICENSE-MIT', 'LICENSE-APACHE', 'Cargo.lock']:
            shutil.copy2(ROOT / name, output / name)
        shutil.copy2(ROOT / 'include/weekend_db.h', output / 'weekend_db.h')
        # All consumers must run after ephemeral build products are gone.
        shutil.rmtree(build)
        header = (output / 'weekend_db.h').read_text()
        declarations = set(re.findall(r'\b(wdb_\w+)\s*\(', header))
        symbols = run(*symbol_command(sys.platform, library, env), collect=True)
        (output / 'symbols.txt').write_text(symbols + '\n')
        exports = set(re.findall(r'\b_?(wdb_\w+)\b', symbols))
        if declarations != exports:
            raise RuntimeError(f'Header/export mismatch, missing {declarations - exports}, extra {exports - declarations}')
        if sys.platform == 'win32':
            dependencies = run(windows_tool('dumpbin.exe', env), '/nologo', '/dependents', library, collect=True)
            (output / 'native-dependencies.txt').write_text(dependencies)
            evidence['native_runtime_policy'] = 'System/MSVC runtime DLLs are listed, not bundled; consumer executables use /MD.'
            evidence['windows_toolchain'] = windows_toolchain_identity(env)
            evidence['windows_tool_sha256'] = {tool: digest(Path(windows_tool(tool, env))) for tool in ['cl.exe', 'dumpbin.exe']}
            (output / 'WINDOWS-RUNTIME.md').write_text(
                'This package contains the project DLL/import library and compiled C/C++ clients. '
                'It does not redistribute Microsoft runtime DLLs. The target machine must have '
                'compatible Microsoft Visual C++ runtime dependencies installed; see native-dependencies.txt. '
                'Clients use /MD; static CRT library builds are rejected.\n\n'
                'Microsoft deployment guidance: https://learn.microsoft.com/en-us/cpp/windows/deploying-visual-cpp-application-by-using-the-vcpp-redistributable-package\n'
                'Runtime selection: https://learn.microsoft.com/en-us/cpp/build/reference/md-mt-ld-use-run-time-library\n')
        for name, version, command, executable in consumer_commands(sys.platform, output, library, env):
            evidence[name + '_compiler'] = run(*version, collect=True)
            run(*command)
            run(executable)
            if sys.platform == 'win32':
                dependencies = run(windows_tool('dumpbin.exe', env), '/nologo', '/dependents', executable, collect=True)
                (output / (name + '-dependencies.txt')).write_text(dependencies)
        run(sys.executable, ROOT / 'tools/ffi_smoke.py', library)
        if args.odin:
            compiler = find_odin(env)
            binding = ROOT / 'bindings/odin/weekend_db.odin'
            if binding.read_text() != odin_bindings.generate(header):
                raise RuntimeError('Odin binding does not match the packaged C ABI header')
            shutil.copytree(ROOT / 'bindings/odin', output / 'bindings/odin')
            shutil.copytree(ROOT / 'examples/odin_package', output / 'odin-consumer')
            evidence['odin'] = {'version': run(compiler, 'version', collect=True).strip(), 'compiler_sha256': digest(Path(compiler))}
            odin_root = Path(run(compiler, 'root', collect=True).strip()).resolve()
            license_file = odin_root / 'LICENSE'
            if license_file.is_file() and license_file.resolve().is_relative_to(odin_root):
                notice = output / 'notices/odin/LICENSE'
                notice.parent.mkdir(parents=True)
                shutil.copy2(license_file, notice)
                evidence['odin']['license_sha256'] = digest(notice)
            else:
                evidence['attribution_inputs_complete'] = False
                evidence['attribution_review_items'].append('Missing Odin core/runtime license from compiler root/LICENSE for consumer-odin')
            command, executable = odin_command(sys.platform, compiler, output, library)
            run(*command)
            with tempfile.TemporaryDirectory(prefix='weekend-package-odin-') as directory:
                run(executable, Path(directory) / 'retained.db', source_id)
            evidence['odin']['verified'] = True
        executables = [output / (name + ('.exe' if sys.platform == 'win32' else ''))
                       for name in ['consumer-c', 'consumer-cpp']]
        if args.odin:
            executables.append(output / ('consumer-odin.exe' if sys.platform == 'win32' else 'consumer-odin'))
        for artifact in [library, *executables]:
            dependencies = run(*dependency_command(sys.platform, artifact, env), collect=True)
            (output / (artifact.name + '-dependencies.txt')).write_text(dependencies)
        # Move, don't merely copy: absolute links back to the original package
        # must fail this test. A sibling temporary directory keeps Windows on
        # the same volume. The ephemeral Cargo build was already removed.
        with tempfile.TemporaryDirectory(prefix='.weekend-relocated-', dir=output.parent) as directory:
            relocated = Path(directory) / 'package'
            output.rename(relocated)
            log_output = relocated
            try:
                if output.exists():
                    raise RuntimeError('Original package path must be absent during relocation check')
                for executable in executables:
                    target = relocated / executable.name
                    if executable.name.startswith('consumer-odin'):
                        run(target, Path(directory) / 'relocated.db', source_id)
                    else:
                        run(target)
                run(sys.executable, ROOT / 'tools/ffi_smoke.py', relocated / filename)
                evidence['relocation_verified'] = True
            finally:
                relocated.rename(output)
                log_output = output
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
        evidence['candidate_eligible'] = candidate_eligible(dirty, evidence['attribution_inputs_complete'])
    finally:
        evidence['sha256'] = {str(path.relative_to(output)): digest(path)
                              for path in output.rglob('*') if path.is_file() and path.name != 'manifest.json'}
        (output / 'manifest.json').write_text(json.dumps(evidence, indent=2) + '\n')
        if build.exists():
            shutil.rmtree(build)
    print(f'Verified private package {output}, candidate eligible {evidence["candidate_eligible"]}')


if __name__ == '__main__':
    main()
