"""Collect attribution inputs for the packages compiled into a native build."""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import tomllib

NOTICE = re.compile(r'^(licen[cs]es?|copying|copyright|notices?)(?:[._-]|$)', re.I)
MAX_NOTICE_BYTES = 8 * 1024 * 1024
MAX_TOTAL_BYTES = 64 * 1024 * 1024


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def package_notices(package, workspace_root, destination):
    root = Path(package['manifest_path']).parent.resolve()
    candidates = {}
    if package.get('license_file'):
        candidates[root / package['license_file']] = root
    # A workspace package gets the project-level declaration too. This is
    # recorded as a separate source boundary, never inferred from an SPDX name.
    boundaries = [root]
    if package.get('source') is None and root.is_relative_to(workspace_root) and root != workspace_root:
        boundaries.append(workspace_root)
    for boundary in boundaries:
        for directory, names, files in os.walk(boundary, followlinks=False):
            current = Path(directory)
            names[:] = sorted(name for name in names if name not in ('.git', 'target')
                              and not (current / name).is_symlink()
                              and not (current / name).resolve().is_relative_to(destination))
            if boundary == workspace_root:
                # Workspace subprojects have their own package inventory.
                names[:] = [name for name in names if NOTICE.match(name)]
            in_notice_directory = any(NOTICE.match(part) for part in current.relative_to(boundary).parts)
            candidates.update((current / name, boundary) for name in files if NOTICE.match(name) or in_notice_directory)
    result = []
    problems = []
    for path, boundary in sorted(candidates.items()):
        # The explicitly declared license-file belongs to its own package root.
        # Parent project files are admitted only through the separate boundary.
        resolved = path.resolve()
        label = os.path.relpath(path, boundary)
        if not resolved.is_relative_to(boundary):
            problems.append('notice path escapes its allowed root: ' + label)
        elif not resolved.is_file():
            problems.append('declared notice is missing or not a file: ' + label)
        else:
            result.append((path, boundary))
    return result, problems


def collect(metadata, package_ids, lockfile, destination, sysroot, compiler):
    destination.mkdir(parents=True, exist_ok=False)
    workspace = Path(metadata['workspace_root']).resolve()
    packages = {package['id']: package for package in metadata['packages']}
    if not package_ids or package_ids - packages.keys():
        raise RuntimeError('Built package IDs must all resolve through Cargo metadata')
    locked = tomllib.loads(lockfile.read_text())['package']
    inventory = {'scope': 'Actual compiler-artifact packages for the library, including build dependencies and proc macros.',
                 'legal_review': 'Unreviewed. Collected declarations and notices do not constitute license approval.',
                 'packages': [], 'review_items': [], 'standard_library': {'compiler': compiler, 'notice_files': []}}
    total = 0

    def copy_notice(source, relative):
        nonlocal total
        size = source.stat().st_size
        if size > MAX_NOTICE_BYTES or total + size > MAX_TOTAL_BYTES:
            raise RuntimeError('Attribution files exceed the bounded copy budget')
        total += size
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
        return {'file': str(relative), 'sha256': sha256(target), 'bytes': size}

    for package_id in sorted(package_ids):
        package = packages[package_id]
        records = [entry for entry in locked if entry['name'] == package['name']
                   and entry['version'] == package['version'] and entry.get('source') == package.get('source')]
        if len(records) != 1:
            raise RuntimeError('Package does not map uniquely to Cargo.lock: ' + package_id)
        checksum = records[0].get('checksum')
        item = {key: package.get(key) for key in ('id', 'name', 'version', 'license', 'license_file', 'source')}
        item['checksum'] = checksum
        item['manifest_sha256'] = sha256(Path(package['manifest_path']))
        item['notice_files'] = []
        # Hash the opaque ID instead of constructing paths from package input.
        folder = Path('packages') / hashlib.sha256(package_id.encode()).hexdigest()
        paths, problems = package_notices(package, workspace, destination.resolve())
        for index, (path, boundary) in enumerate(paths):
            file = copy_notice(path, folder / str(index) / path.name)
            file['source_relative_path'] = str(path.relative_to(boundary))
            file['source_scope'] = 'package' if boundary == Path(package['manifest_path']).parent.resolve() else 'workspace'
            item['notice_files'].append(file)
        if not package.get('license') and not package.get('license_file'):
            problems.append('no declared license expression or license-file')
        if not item['notice_files']:
            problems.append('no license or notice text found')
        if (package.get('source') or '').startswith('registry+') and not checksum:
            problems.append('registry package has no Cargo.lock checksum')
        inventory['review_items'].extend({'package': package_id, 'reason': reason} for reason in problems)
        inventory['packages'].append(item)
    standard = sysroot / 'share/doc/rust/COPYRIGHT-library.html'
    if standard.is_file() and standard.resolve().is_relative_to(sysroot.resolve()):
        inventory['standard_library']['notice_files'].append(copy_notice(standard, Path('rust-stdlib/COPYRIGHT-library.html')))
    else:
        inventory['review_items'].append({'package': 'rust-stdlib', 'reason': 'matching toolchain COPYRIGHT-library.html is missing'})
    inventory['attribution_inputs_complete'] = not inventory['review_items']
    (destination / 'inventory.json').write_text(json.dumps(inventory, indent=2) + '\n')
    return inventory
