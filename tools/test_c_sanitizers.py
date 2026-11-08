#!/usr/bin/env python3
"""Check that inherited instrumentation settings cannot disable this profile."""
import os
from pathlib import Path
import unittest
from unittest.mock import patch

from c_sanitizers import environment


class EnvironmentTests(unittest.TestCase):
    def test_hostile_inherited_flags_and_suppressions_are_replaced(self):
        inherited = {"RUSTFLAGS": "-Copt-level=3", "RUSTDOCFLAGS": "bad",
                     "CARGO_ENCODED_RUSTFLAGS": "-Zsanitizer=thread",
                     "CARGO_ENCODED_RUSTDOCFLAGS": "bad", "RUSTC": "other-rustc",
                     "RUSTDOC": "other-rustdoc", "RUSTC_WRAPPER": "wrapper",
                     "RUSTC_WORKSPACE_WRAPPER": "wrapper", "LSAN_OPTIONS": "suppressions=all",
                     "ASAN_OPTIONS": "detect_leaks=0:halt_on_error=0:suppressions=all"}
        with patch.dict(os.environ, inherited, clear=True):
            env = environment(Path("/tmp/build"), ["-Zsanitizer=address", "-C", "link-arg=a b"])
        for key in inherited.keys() - {"ASAN_OPTIONS", "CARGO_ENCODED_RUSTFLAGS"}:
            self.assertNotIn(key, env)
        self.assertEqual(env["CARGO_ENCODED_RUSTFLAGS"], "-Zsanitizer=address\x1f-C\x1flink-arg=a b")
        self.assertNotIn("suppressions", env["ASAN_OPTIONS"])
        self.assertIn("halt_on_error=1", env["ASAN_OPTIONS"])
        self.assertEqual(env["CARGO_INCREMENTAL"], "0")


if __name__ == "__main__":
    unittest.main()
