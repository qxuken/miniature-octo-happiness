#!/usr/bin/env python3
"""Build and verify a private native package, with provenance and no publication."""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
sys.path.insert(0, str(Path(__file__).resolve().parent))
import release_notices
import windows_consumers
import odin_bindings
import rust_source_package

ROOT = Path(__file__).resolve().parent.parent


def capture(*command):
    return subprocess.check_output(command, cwd=ROOT, text=True).strip()


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def build_configuration(env):
    """Hash overrides/configs, never copy their potentially secret contents."""
    names = {'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC', 'RUSTC_WRAPPER',
             'RUSTC_WORKSPACE_WRAPPER', 'RUSTUP_TOOLCHAIN', 'CARGO_HOME',
             'CC', 'CXX', 'CFLAGS', 'CXXFLAGS', 'LDFLAGS', 'CARGO_INCREMENTAL'}
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


def xtask_helper(action):
    """Retained leaf policies/checkers; Rust owns the package lifecycle."""
    request = json.load(sys.stdin)
    if action == 'configuration':
        result = build_configuration(os.environ)
    elif action == 'environment':
        result = windows_consumers.compiler_environment(Path(request['directory']))
    elif action == 'compiler-cfg':
        require_unwind(request['cfg'])
        if sys.platform == 'win32': require_dynamic_crt(request['cfg'])
        result = {'panic_strategy': 'unwind'}
    elif action == 'plan':
        output = Path(request['output']); env = os.environ
        library = output / platform_artifacts(sys.platform)[0]
        consumers = consumer_commands(sys.platform, output, library, env)
        artifacts = [library, *[c[3] for c in consumers]]
        result = {'artifacts': platform_artifacts(sys.platform),
                  'library_flags': library_link_flags(sys.platform),
                  'symbols': symbol_command(sys.platform, library, env),
                  'consumers': [dict(zip(['name', 'version', 'command', 'executable'], c)) for c in consumers]}
        if request['odin']:
            compiler = find_odin(env)
            command, executable = odin_command(sys.platform, compiler, output, library)
            result['odin'] = {'compiler': compiler, 'command': command, 'executable': executable}
            artifacts.append(executable)
        result['dependencies'] = [{'artifact': p.name, 'command': dependency_command(sys.platform, p, env)} for p in artifacts]
        if sys.platform == 'win32':
            result['windows_toolchain'] = windows_toolchain_identity(env)
            result['windows_tool_sha256'] = {tool: digest(Path(windows_tool(tool, env))) for tool in ['cl.exe', 'dumpbin.exe']}
        result['runtime_dependencies'] = dependency_command(sys.platform, library, env)
    elif action == 'notices':
        result = release_notices.collect(request['metadata'], set(request['packages']),
            Path(request['lockfile']), Path(request['destination']), Path(request['sysroot']), request['compiler'])
    elif action == 'rust-source':
        output = Path(request['output']); commands = []
        def run(*command, collect=False, cwd=None, timeout=None):
            command = list(map(str, command)); commands.append(command)
            with (output / 'checks.log').open('a') as log:
                if collect:
                    child = subprocess.run(command, cwd=cwd or ROOT, stdout=subprocess.PIPE,
                        stderr=subprocess.STDOUT, text=True, timeout=timeout)
                    log.write(child.stdout); child.check_returncode(); return child.stdout
                subprocess.run(command, cwd=cwd or ROOT, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=timeout)
        info, metadata, packages = rust_source_package.qualify(ROOT, output, request['tracked'],
            request['source_id'], run, request['image'])
        result = {'info': info, 'metadata': metadata, 'packages': sorted(packages), 'commands': commands}
    else:
        raise ValueError('unknown private xtask helper action')
    json.dump(result, sys.stdout, default=lambda value: str(value) if isinstance(value, Path) else None)
    print()


def main():
    if len(sys.argv) == 3 and sys.argv[1] == '--xtask-helper':
        xtask_helper(sys.argv[2]); return
    # Compatibility only. All package orchestration lives in the private crate.
    env = os.environ.copy(); env['WEEKEND_PYTHON'] = sys.executable
    raise SystemExit(subprocess.call(['cargo', 'xtask', 'package', *sys.argv[1:]], cwd=ROOT, env=env))


if __name__ == '__main__':
    main()
