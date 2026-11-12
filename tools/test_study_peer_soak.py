import contextlib
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import study_peer_soak as soak


class FakePeer:
    def __init__(self, acknowledged_total):
        self.count = 0
        self.total = acknowledged_total

    def state(self):
        return [4, 3, [201], [11], [2, 3]]

    def acknowledgments(self):
        self.count += 1
        return self.count, f"peer {self.count} ack {self.total}", self.total


class QualificationOracles(unittest.TestCase):
    def test_converged_state_and_increasing_counts_do_not_replace_covering_ack(self):
        clock = [0.0]

        def advance(seconds):
            clock[0] += seconds

        with patch.object(soak.time, "monotonic", side_effect=lambda: clock[0]), patch.object(soak.time, "sleep", side_effect=advance):
            with self.assertRaises(TimeoutError):
                soak.converge(FakePeer(0), FakePeer(0), [3, [201], [11], [2, 3]])
            states, counts = soak.converge(FakePeer(3), FakePeer(3), [3, [201], [11], [2, 3]])
        self.assertEqual(states, [[4, 3, [201], [11], [2, 3]]] * 2)
        self.assertEqual(counts, [3, 3])

    def test_existing_output_and_invalid_workload_reject_before_external_work(self):
        with tempfile.TemporaryDirectory() as temp:
            sentinel = Path(temp) / "existing"
            sentinel.write_bytes(b"preserve caller data")
            directory = Path(temp) / "existing-dir"
            directory.mkdir()
            dangling = Path(temp) / "dangling"
            dangling.symlink_to(Path(temp) / "must-not-create")
            for path, args in [(sentinel, []), (directory, []), (dangling, []), (Path(temp) / "new", ["--rounds", "0"])]:
                with patch.object(soak.platform, "system", return_value="Darwin"), patch("sys.argv", ["study_peer_soak.py", str(path), *args]), patch.object(soak.subprocess, "check_output", side_effect=AssertionError("external work before validation")), contextlib.redirect_stderr(io.StringIO()):
                    with self.assertRaises(SystemExit) as raised:
                        soak.main()
                    self.assertEqual(raised.exception.code, 2)
            self.assertEqual(sentinel.read_bytes(), b"preserve caller data")
            self.assertFalse((Path(temp) / "new").exists())
            self.assertFalse((Path(temp) / "must-not-create").exists())
            self.assertTrue(dangling.is_symlink())


if __name__ == "__main__":
    unittest.main()
