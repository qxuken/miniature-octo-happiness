#!/usr/bin/env python3
"""Deterministic checks for release configuration validation and redaction."""
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import release
import release_notices


class ReleaseConfiguration(unittest.TestCase):
    def test_only_one_unwind_strategy_is_accepted(self):
        release.require_unwind('target_os="linux"\npanic="unwind"\n')
        for cfg in ['', 'panic="abort"\n', 'panic="unwind"\npanic="abort"\n']:
            with self.assertRaises(RuntimeError):
                release.require_unwind(cfg)

    def test_configuration_hashes_detect_changes_without_copying_secrets(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            workspace = root / 'workspace'
            cargo_home = root / 'cargo-home'
            (workspace / '.cargo').mkdir(parents=True)
            cargo_home.mkdir()
            config = workspace / '.cargo/config.toml'
            config.write_text('[registry]\ntoken="private-credential"\n')
            (cargo_home / 'credentials.toml').write_text('token="another-credential"\n')
            env = {'CARGO_HOME': str(cargo_home), 'RUSTFLAGS': '--cfg private_configuration',
                   'CARGO_PROFILE_RELEASE_PANIC': 'abort', 'CARGO_REGISTRIES_EXAMPLE_TOKEN': 'env-credential'}
            with patch.object(release, 'ROOT', workspace):
                first = release.build_configuration(env)
                encoded = json.dumps(first)
                for secret in ['private-credential', 'another-credential', 'env-credential', 'private_configuration']:
                    self.assertNotIn(secret, encoded)
                self.assertIn(str(config.resolve()), first['cargo_config_sha256'])
                self.assertNotIn(str(cargo_home / 'credentials.toml'), first['cargo_config_sha256'])
                config.write_text('[build]\nrustflags=[]\n')
                self.assertNotEqual(first, release.build_configuration(env))
                second = release.build_configuration(env)
                env['CARGO_PROFILE_RELEASE_PANIC'] = 'unwind'
                self.assertNotEqual(second, release.build_configuration(env))


class NativePackageCommands(unittest.TestCase):
    def test_windows_packages_dll_and_import_library(self):
        self.assertEqual(release.platform_artifacts('win32'), ('weekend_db.dll', 'weekend_db.dll.lib'))
        self.assertEqual(release.platform_artifacts('darwin'), ('libweekend_db.dylib',))
        self.assertEqual(release.platform_artifacts('linux'), ('libweekend_db.so',))
        with self.assertRaises(RuntimeError):
            release.platform_artifacts('unknown')

    def test_windows_clients_use_packaged_header_and_linker_input(self):
        output = Path('package with spaces')
        library = output / 'weekend_db.dll'
        with patch.object(release.shutil, 'which', side_effect=lambda name, path: 'msvc/' + name):
            commands = release.consumer_commands('win32', output, library, {'Path': 'toolchain'})
            symbols = release.symbol_command('win32', library, {'Path': 'toolchain'})
        self.assertEqual(symbols, ['msvc/dumpbin.exe', '/nologo', '/exports', str(library)])
        for (name, version, command, executable), language in zip(commands, ['/TC', '/TP']):
            self.assertEqual(version, ['msvc/cl.exe', '/?'])
            self.assertIn(language, command)
            self.assertIn('/MD', command)
            self.assertIn('/I' + str(output), command)
            self.assertNotIn('/I' + str(release.ROOT / 'include'), command)
            self.assertEqual(command[command.index('/link') + 1:], [str(output / 'weekend_db.dll.lib')])
            self.assertEqual(executable, output / (name + '.exe'))
        with patch.object(release.shutil, 'which', return_value=None):
            with self.assertRaises(RuntimeError):
                release.symbol_command('win32', library, {})

    def test_unix_exact_artifact_commands_are_preserved(self):
        output = Path('package')
        for platform in ['darwin', 'linux']:
            library = output / release.platform_artifacts(platform)[0]
            commands = release.consumer_commands(platform, output, library, {})
            for name, version, command, executable in commands:
                self.assertIn(str(library), command)
                self.assertIn('-Wl,-rpath,' + ('@loader_path' if platform == 'darwin' else '$ORIGIN'), command)
                self.assertEqual(executable, output / name)
                self.assertEqual(version[-1], '--version')

    def test_relocatable_library_and_odin_commands(self):
        self.assertIn('link-arg=-Wl,-install_name,@rpath/libweekend_db.dylib', release.library_link_flags('darwin'))
        self.assertIn('link-arg=-Wl,-soname,libweekend_db.so', release.library_link_flags('linux'))
        self.assertEqual(release.library_link_flags('win32'), [])
        output = Path('/package with spaces')
        for platform in ['darwin', 'linux', 'win32']:
            library = output / release.platform_artifacts(platform)[0]
            command, executable = release.odin_command(platform, 'odin', output, library)
            self.assertIn('-collection:weekend=' + str(output / 'bindings'), command)
            linked = 'weekend_db.dll.lib' if platform == 'win32' else library.name
            self.assertIn('-define:WEEKEND_DB_LIBRARY=../../' + linked, command)
            self.assertEqual(executable.parent, output)
            if platform != 'win32':
                self.assertIn('-extra-linker-flags:-Wl,-rpath,' + ('@loader_path' if platform == 'darwin' else '$ORIGIN'), command)
        with patch.object(release.shutil, 'which', return_value=None):
            with self.assertRaisesRegex(RuntimeError, 'cannot be skipped'):
                release.find_odin({})

    def test_odin_binding_exactly_tracks_header_signatures_and_constants(self):
        header = (release.ROOT / 'include/weekend_db.h').read_text()
        actual = (release.ROOT / 'bindings/odin/weekend_db.odin').read_text()
        self.assertEqual(release.odin_bindings.generate(header), actual)
        changed = header.replace('wdb_tx_commit(wdb_tx *tx, uint64_t *revision)',
                                 'wdb_tx_commit(wdb_tx *tx, int32_t *revision)')
        self.assertNotEqual(release.odin_bindings.generate(changed), actual)
        self.assertNotEqual(release.odin_bindings.generate(header.replace('WDB_ABI_VERSION 1u', 'WDB_ABI_VERSION 2u')), actual)
        with self.assertRaises(ValueError):
            release.odin_bindings.generate(header + '\nuint64_t wdb_unknown(void);')

    def test_windows_toolchain_versions_are_required_and_case_insensitive(self):
        identity = release.windows_toolchain_identity({'VCToolsVersion': '14.44', 'WindowsSDKVersion': '10.0.26100.0', 'SECRET': 'not evidence'})
        self.assertEqual(identity, {'vctoolsversion': '14.44', 'windowssdkversion': '10.0.26100.0'})
        with self.assertRaises(RuntimeError):
            release.windows_toolchain_identity({'VCToolsVersion': '14.44'})

    def test_runtime_configuration_and_candidate_gates_are_distinct(self):
        release.require_dynamic_crt('panic="unwind"\ntarget_feature="sse2"\n')
        with self.assertRaises(RuntimeError):
            release.require_dynamic_crt('panic="unwind"\ntarget_feature="crt-static"\n')
        self.assertTrue(release.candidate_eligible('', True))
        # Both packages may verify at runtime; neither can be a candidate.
        self.assertFalse(release.candidate_eligible(' M source', True))
        self.assertFalse(release.candidate_eligible('', False))


class Attribution(unittest.TestCase):
    def test_built_packages_only_and_missing_inputs_are_review_gates(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            package = root / 'package'
            package.mkdir()
            manifest = package / 'Cargo.toml'
            manifest.write_text('[package]\nname="sample"\n')
            (package / 'LICENSE-MIT').write_text('example attribution text')
            source = 'registry+https://example.invalid/index'
            item = {'id': 'opaque-sample-id', 'name': 'sample', 'version': '1.0.0',
                    'manifest_path': str(manifest), 'source': source, 'license': 'MIT', 'license_file': None}
            metadata = {'workspace_root': str(root / 'workspace'), 'packages': [item, dict(item, id='unused-app')]}
            lock = root / 'Cargo.lock'
            lock.write_text('[[package]]\nname="sample"\nversion="1.0.0"\nsource="' + source + '"\nchecksum="abc123"\n')
            sysroot = root / 'sysroot'
            standard = sysroot / 'share/doc/rust/COPYRIGHT-library.html'
            standard.parent.mkdir(parents=True)
            standard.write_text('standard library attribution')
            inventory = release_notices.collect(metadata, {'opaque-sample-id'}, lock, root / 'output', sysroot, 'test rustc')
            self.assertTrue(inventory['attribution_inputs_complete'])
            self.assertEqual(len(inventory['packages']), 1)
            entry = inventory['packages'][0]
            self.assertEqual(entry['checksum'], 'abc123')
            copied = root / 'output' / entry['notice_files'][0]['file']
            self.assertEqual(copied.read_text(), 'example attribution text')
            self.assertEqual(entry['notice_files'][0]['sha256'], release.digest(copied))
            standard.unlink()
            item['license'] = None
            inventory = release_notices.collect(metadata, {'opaque-sample-id'}, lock, root / 'missing', sysroot, 'test rustc')
            self.assertFalse(inventory['attribution_inputs_complete'])
            self.assertEqual(len(inventory['review_items']), 2)

    def test_declared_paths_and_symlinks_cannot_copy_outside_package(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            package = root / 'package'
            package.mkdir()
            outside = root / 'secret'
            outside.write_text('must not be copied')
            item = {'manifest_path': str(package / 'Cargo.toml'), 'source': 'registry+example', 'license_file': '../secret'}
            paths, problems = release_notices.package_notices(item, root / 'workspace', root / 'output')
            self.assertEqual(paths, [])
            self.assertEqual(len(problems), 1)
            item['license_file'] = str(outside)
            paths, problems = release_notices.package_notices(item, root / 'workspace', root / 'output')
            self.assertEqual(paths, [])
            self.assertEqual(len(problems), 1)
            try:
                (package / 'LICENSE').symlink_to(outside)
            except OSError:
                return  # Windows runners may lack symlink privileges.
            item['license_file'] = None
            paths, problems = release_notices.package_notices(item, root / 'workspace', root / 'output')
            self.assertEqual(paths, [])
            self.assertEqual(len(problems), 1)


if __name__ == '__main__':
    unittest.main()
