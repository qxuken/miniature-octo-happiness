"""Private source profile: explicit source inventory and relative dependency closure."""
import hashlib
import json
import re
from pathlib import Path
import shutil
import subprocess
import uuid
import tempfile
import tomllib

CONSUMER = Path('examples/rust_package/Cargo.toml')
# Development packages can include only these reviewed, newly added files;
# arbitrary untracked files (including credentials) are never copied.
CONSUMER_FILES = [CONSUMER, CONSUMER.parent / 'Cargo.lock', CONSUMER.parent / 'src/main.rs']


def copy_sources(root, destination, tracked):
    names = set(tracked) | {str(path) for path in CONSUMER_FILES}
    inventory = {}
    for name in sorted(names):
        relative = Path(name)
        if relative.is_absolute() or '..' in relative.parts:
            raise RuntimeError('Invalid source inventory path: ' + name)
        if any(part in {'.git', 'target', '__pycache__'} for part in relative.parts):
            continue
        source = root / relative
        if source.is_symlink() or not source.resolve().is_relative_to(root.resolve()) or not source.is_file():
            raise RuntimeError('Source inventory requires regular in-tree files: ' + name)
        if relative.name == 'Cargo.toml':
            manifest = tomllib.loads(source.read_text())
            if 'package' in manifest and manifest['package'].get('publish') is not False:
                raise RuntimeError('Source Cargo package must declare publish=false: ' + name)
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)
        inventory[relative.as_posix()] = hashlib.sha256(target.read_bytes()).hexdigest()
    return inventory


def validate_metadata(metadata, source):
    source = source.resolve()
    local = []
    for package in metadata['packages']:
        origin = package.get('source')
        manifest = Path(package['manifest_path']).resolve()
        if origin is None:
            if not manifest.is_relative_to(source):
                raise RuntimeError('Path dependency escapes delivered source: ' + str(manifest))
            if package.get('publish') != []:
                raise RuntimeError('Private source package must have publish=false: ' + package['name'])
            local.append(str(manifest.relative_to(source)))
        elif not origin.startswith('registry+'):
            raise RuntimeError('Unqualified non-registry dependency source: ' + origin)
    if not local:
        raise RuntimeError('No local source dependencies found')
    return local


def references_original(text, original, allowed):
    # Rust dep-info uses Make escaping; normalize separators for native Windows.
    text = text.replace('\\ ', ' ').replace('\\:', ':').replace('\\', '/')
    for prefix in sorted((Path(path).resolve().as_posix() for path in allowed), key=len, reverse=True):
        text = re.sub(re.escape(prefix) + r'(?=/|$)', '<delivered-or-registry>', text, flags=re.M)
    return Path(original).resolve().as_posix() in text


def build_commands(source, target):
    manifest = str(source / CONSUMER)
    return (['cargo', 'metadata', '--offline', '--locked', '--format-version=1', '--manifest-path', manifest],
            ['cargo', 'build', '--offline', '--locked', '--release', '--message-format=json-render-diagnostics',
             '--manifest-path', manifest, '--target-dir', str(target)])


def isolated_command(source, original, source_id, image, name):
    script = '''set -eu
[ ! -e "$ORIGINAL_SOURCE" ]
rustc -Vv
cargo build --locked --release --manifest-path /package-source/examples/rust_package/Cargo.toml --target-dir /tmp/rust-source-build
/tmp/rust-source-build/release/private-rust-consumer "$WEEKEND_DB_SOURCE_ID"
'''
    return ['docker', 'run', '--rm', '--name', name, '--memory=2g', '--pids-limit=256', '--cpus=2',
            '--mount', f'type=bind,source={source},target=/package-source,readonly',
            '--env', 'CARGO_INCREMENTAL=0', '--env', 'WEEKEND_DB_SOURCE_ID=' + source_id,
            '--env', 'ORIGINAL_SOURCE=' + str(original), image, 'sh', '-c', script]


def qualify(root, output, tracked, source_id, run, image=None):
    source = output / 'rust-source'
    inventory = copy_sources(root, source, tracked)
    info = {'source_files_sha256': inventory, 'registry_dependencies_vendored': False,
            'native_compile_verified': False, 'linux_isolation_verified': False}
    with tempfile.TemporaryDirectory(prefix='weekend-rust-source-') as directory:
        metadata_command, build_command = build_commands(source, Path(directory))
        metadata = next(json.loads(line) for line in run(*metadata_command, collect=True, cwd=source).splitlines() if line.startswith('{'))
        info['local_manifests'] = validate_metadata(metadata, source)
        # Independent consumer workspaces do not inherit the repository's
        # license files through Cargo metadata. Add exact project-license text
        # to each of our local MIT/Apache packages without changing source files.
        info['added_project_license_copies'] = {}
        for package in metadata['packages']:
            if package.get('source') is None and package.get('license') == 'MIT OR Apache-2.0':
                for name in ['LICENSE-MIT', 'LICENSE-APACHE']:
                    target = Path(package['manifest_path']).parent / name
                    if not target.exists():
                        shutil.copy2(source / name, target)
                        info['added_project_license_copies'][str(target.relative_to(source))] = hashlib.sha256(target.read_bytes()).hexdigest()
        built = run(*build_command, collect=True, cwd=source)
        artifacts = [json.loads(line) for line in built.splitlines() if line.startswith('{')]
        packages = {item['package_id'] for item in artifacts if item.get('reason') == 'compiler-artifact'}
        consumer = next(Path(item['executable']) for item in artifacts
                        if item.get('reason') == 'compiler-artifact' and item['target']['name'] == 'private-rust-consumer')
        if not consumer.resolve().is_relative_to(Path(directory).resolve()) or not consumer.is_file():
            raise RuntimeError('Consumer executable escapes isolated build target')
        # Compiler dependency files must never point back at the original tree.
        for path in Path(directory).rglob('*.d'):
            allowed = [source.resolve(), *[Path(p['manifest_path']).parent.resolve() for p in metadata['packages'] if p.get('source')]]
            if references_original(path.read_text(), root, allowed):
                raise RuntimeError('Compiler dependency file references original source: ' + str(path))
        run(consumer, source_id, cwd=source)
        info['native_compile_verified'] = True
        info['consumer_sha256'] = hashlib.sha256(consumer.read_bytes()).hexdigest()
    if image:
        name = 'weekend-source-' + uuid.uuid4().hex
        try:
            image_id = run('docker', 'image', 'inspect', '--format={{.Id}}', image, collect=True).strip()
            if not re.fullmatch(r'sha256:[0-9a-f]{64}', image_id):
                raise RuntimeError('Docker did not identify an immutable local image')
            run(*isolated_command(source, root, source_id, image_id, name), timeout=300)
            info['docker_image'] = image_id
            info['linux_isolation_verified'] = True
        finally:
            subprocess.run(['docker', 'rm', '-f', name], check=False, timeout=30,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return info, metadata, packages
