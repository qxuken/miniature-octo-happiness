#!/usr/bin/env python3
import tempfile
from pathlib import Path
import unittest
from unittest.mock import patch

import fuzz


class EvidenceTests(unittest.TestCase):
    def test_requested_runs_and_failed_builds_are_not_completed_runs(self):
        self.assertIsNone(fuzz.completed_runs('cargo fuzz -runs=1000\nerror: failed to compile\n'))
        self.assertIsNone(fuzz.completed_runs('Done 1000 runs in fake second(s)\n'))
        self.assertEqual(fuzz.completed_runs('Done 0 runs in 0 second(s)\n'), 0)
        self.assertEqual(fuzz.completed_runs('#538 DONE\nDone 538 runs in 21 second(s)\n'), 538)

    def test_input_changes_are_visible_and_corpus_changes_are_excluded(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for path in ['src/engine.rs', 'fuzz/src/peer.rs', 'fuzz/target/generated.rs',
                         'fuzz/corpus/input.rs', 'apps/peer/src/parser.rs']:
                p = root / path
                p.parent.mkdir(parents=True, exist_ok=True)
                p.write_text('before')
            with patch.object(fuzz, 'ROOT', root):
                baseline = fuzz.source_hashes()
                self.assertEqual(set(baseline), {'src/engine.rs', 'fuzz/src/peer.rs', 'apps/peer/src/parser.rs'})
                (root / 'fuzz/corpus/input.rs').write_text('after')
                self.assertEqual(fuzz.source_hashes(), baseline)
                (root / 'apps/peer/src/parser.rs').write_text('after')
                self.assertNotEqual(fuzz.source_hashes(), baseline)


if __name__ == '__main__':
    unittest.main()
