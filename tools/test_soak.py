"""Independent soak-runner boundary and lifecycle-result controls."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import soak


def records(count=9):
    rows = [dict(round=n, elapsed_ms=n * 10, revision=2 + 65*n + max(0, n-8),
                 reads=n*2, retained=min(n, 8), live_bytes=1, peak_bytes=2,
                 append_bytes=16384, compacted_bytes=8192) for n in range(1, count+1)]
    return rows + [dict(complete=True, rounds=count, elapsed_ms=count*10, reads=count*2)]


class SoakTests(unittest.TestCase):
    def test_round_oracle_accepts_short_and_release_transition(self):
        for count in [1, 8, 9]:
            rows = records(count)
            self.assertEqual(soak.summary(rows, count), rows[-1])

    def test_incomplete_inconsistent_and_forged_round_records_rejected(self):
        bad = [[], [{'complete': True}], records()[:-1], records()[1:], records()[::-1]]
        changes = {'round': 3, 'elapsed_ms': -1, 'revision': 1, 'reads': 0,
                   'retained': 0, 'live_bytes': 0, 'peak_bytes': soak.CAP+1,
                   'append_bytes': 32*1024*1024, 'compacted_bytes': 16385}
        for name, value in changes.items():
            for replacement in [value, True, '1']:
                rows = records()
                rows[0][name] = replacement
                bad.append(rows)
        for name, value in [('rounds', 1), ('reads', 1), ('elapsed_ms', -1),
                            ('rounds', True), ('complete', 1)]:
            rows = records()
            rows[-1][name] = value
            bad.append(rows)
        for rows in bad:
            with self.subTest(rows=rows), self.assertRaises((RuntimeError, AttributeError)):
                soak.summary(rows, 9)
        with self.assertRaises(RuntimeError):
            soak.summary(records(), 8)

    def test_existing_and_dangling_output_rejected_before_external_work(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            file = root/'file'
            file.write_bytes(b'preserve')
            directory = root/'directory'
            directory.mkdir()
            link = root/'link'
            link.symlink_to(root/'missing')
            with patch.object(soak.helpers, 'run', side_effect=AssertionError('no external work')):
                for output in [file, directory, link]:
                    with self.assertRaises((ValueError, FileExistsError)):
                        soak.main([str(output)])
            self.assertEqual(file.read_bytes(), b'preserve')
            self.assertFalse((root/'missing').exists())
            self.assertEqual(list(directory.iterdir()), [])

    def test_setup_failure_keeps_terminal_evidence(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp)/'run'
            with patch.object(soak.helpers, 'run', side_effect=RuntimeError('probe failure')):
                with self.assertRaisesRegex(RuntimeError, 'probe failure'):
                    soak.main([str(output)])
            saved = json.loads((output/'evidence.json').read_text())
            self.assertFalse(saved['verified'])
            self.assertIn('probe failure', saved['failure'])

    def child_case(self, tmp, contents, code=0, env=None):
        command = [sys.executable, '-c', f'import sys; print({contents!r}); sys.exit({code})']
        evidence = {'verified': False}
        soak.execute(command, Path(tmp), evidence, 1, 9, env or os.environ.copy())
        return evidence

    def test_actual_complete_child_has_exact_round_oracle(self):
        with tempfile.TemporaryDirectory() as tmp:
            text = '\n'.join(json.dumps(r) for r in records())
            result = self.child_case(tmp, text)
            self.assertEqual(result['last_record']['rounds'], 9)
            self.assertEqual(result['returncode'], 0)
            self.assertEqual(set(result['logs_sha256']), {'rounds.jsonl', 'stderr.log'})

    def test_zero_exit_incomplete_and_malformed_children_rejected(self):
        for text in ['{"complete":true}', 'not-json']:
            with tempfile.TemporaryDirectory() as tmp, self.subTest(text=text):
                with self.assertRaises((RuntimeError, json.JSONDecodeError)):
                    self.child_case(tmp, text)
        with tempfile.TemporaryDirectory() as tmp, self.assertRaises(RuntimeError):
            self.child_case(tmp, '\n'.join(json.dumps(r) for r in records()), code=1)

    def test_failed_rss_probe_terminates_and_reaps_real_child(self):
        with tempfile.TemporaryDirectory() as tmp:
            evidence = {'verified': False}
            command = [sys.executable, '-c', 'import time; time.sleep(30)']
            result = subprocess.CompletedProcess(['ps'], 1, '', 'failure')
            with patch.object(soak.subprocess, 'run', return_value=result):
                with self.assertRaisesRegex(RuntimeError, 'RSS probe failed'):
                    soak.execute(command, Path(tmp), evidence, 1, 1, os.environ.copy())
            self.assertNotEqual(evidence['returncode'], 0)
            with self.assertRaises(ProcessLookupError):
                os.kill(evidence['pid'], 0)

    def test_rss_probe_timeout_preserves_child_status(self):
        with tempfile.TemporaryDirectory() as tmp:
            evidence = {'verified': False}
            with patch.object(soak.subprocess, 'run', side_effect=subprocess.TimeoutExpired(['ps'], 5)):
                with self.assertRaises(subprocess.TimeoutExpired):
                    soak.execute([sys.executable, '-c', 'import time; time.sleep(30)'],
                                 Path(tmp), evidence, 1, 1, os.environ.copy())
            self.assertNotEqual(evidence['returncode'], 0)
            with self.assertRaises(ProcessLookupError):
                os.kill(evidence['pid'], 0)


if __name__ == '__main__':
    unittest.main()
