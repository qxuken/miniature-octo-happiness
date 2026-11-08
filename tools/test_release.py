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
