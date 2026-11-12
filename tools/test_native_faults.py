import contextlib
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import native_faults


class NativeFaultRunnerAdmission(unittest.TestCase):
    def test_existing_and_invalid_paths_reject_before_external_work(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            sentinel = root / "sentinel"
            sentinel.write_bytes(b"do not change caller data")
            directory = root / "existing"
            directory.mkdir()
            dangling = root / "dangling"
            target = root / "must-not-create"
            dangling.symlink_to(target)
            for path in [sentinel, directory, dangling, root / "absent" / "child"]:
                with self.subTest(path=path), patch("sys.argv", ["native_faults.py", "--output", str(path)]), patch.object(native_faults.subprocess, "check_output", side_effect=AssertionError("external work before admission")), contextlib.redirect_stderr(io.StringIO()):
                    with self.assertRaises(SystemExit) as result:
                        native_faults.main()
                    self.assertEqual(result.exception.code, 2)
            self.assertEqual(sentinel.read_bytes(), b"do not change caller data")
            self.assertTrue(dangling.is_symlink())
            self.assertFalse(target.exists())
            self.assertFalse((root / "absent").exists())


if __name__ == "__main__":
    unittest.main()
