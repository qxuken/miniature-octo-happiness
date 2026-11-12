"""Safety and comparator-oracle checks; no remote services or existing data used."""
import argparse
import importlib.util
from pathlib import Path
import sqlite3
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("baseline", Path(__file__).with_name("run.py"))
baseline = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(baseline)


class BaselineTests(unittest.TestCase):
    def test_existing_outputs_and_symlinks_rejected_before_sqlite(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            existing = root / "existing.sqlite"
            db = sqlite3.connect(existing)
            db.execute("CREATE TABLE sentinel(value)")
            db.execute("INSERT INTO sentinel VALUES(42)")
            db.commit()
            db.close()
            before = existing.read_bytes()
            link = root / "dangling"
            link.symlink_to(root / "missing")
            for candidate in (existing, root, link):
                args = argparse.Namespace(output=str(candidate), revision="known", commits=8,
                    keys=5, batch=3, payload=32, retain_every=3, reopens=2, rounds=1)
                with patch.object(baseline, "command") as command, patch.object(
                        baseline.sqlite3, "connect", side_effect=AssertionError("must not open SQLite")):
                    command.return_value.stdout = "0" * 40
                    with self.assertRaises(ValueError):
                        baseline.run(args)
            self.assertEqual(existing.read_bytes(), before)
            self.assertFalse((root / "missing").exists())

    def test_invalid_workload_rejects_before_output_creation(self):
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp) / "new"
            args = argparse.Namespace(output=str(output), revision="known", commits=8,
                keys=5, batch=6, payload=32, retain_every=3, reopens=2, rounds=1)
            with patch.object(baseline, "command", side_effect=AssertionError("must not execute")):
                with self.assertRaises(ValueError):
                    baseline.run(args)
            self.assertFalse(output.exists())

    def test_copy_destination_rejected_without_open_or_mutation(self):
        with tempfile.TemporaryDirectory() as temp:
            target = Path(temp) / "sentinel"
            target.write_bytes(b"preserve-existing-output")
            with patch.object(baseline, "connect", side_effect=AssertionError("must not open")):
                with self.assertRaises(ValueError):
                    baseline.verified_copy(Path(temp) / "absent-source", target, {}, ({}, {}))
            self.assertEqual(target.read_bytes(), b"preserve-existing-output")

    def test_history_survives_reopen_gc_vacuum_backup_and_restore(self):
        work = dict(commits=8, keys=5, batch=3, payload=32, retain_every=3, reopens=2)
        expected = baseline.oracle(work)
        self.assertEqual(list(expected[1]), [3, 6, 8])
        with tempfile.TemporaryDirectory() as temp:
            result = baseline.sqlite_run(Path(temp) / "sqlite", work, expected)
            self.assertEqual(result["oracle_digest"], baseline.digest(*expected, 8))
            self.assertEqual(result["version_rows_before_gc"], 29)
            self.assertLess(result["version_rows_after_gc"], 29)
            self.assertEqual(result["sizes_closed"]["-wal"], 0)
            self.assertEqual(result["sizes_checkpointed"]["-wal"], 0)
            self.assertEqual(len(result["pin_ns"]), 3)

    def test_full_oracle_rejects_noncanonical_keys_and_corrupted_state(self):
        work = dict(commits=1, keys=1, batch=1, payload=16, retain_every=1, reopens=1)
        expected = baseline.oracle(work)
        db = sqlite3.connect(":memory:", isolation_level=None)
        self.addCleanup(db.close)
        db.executescript("""CREATE TABLE versions(k BLOB,epoch INTEGER,payload BLOB,
            PRIMARY KEY(k,epoch)); CREATE TABLE head(epoch INTEGER);
            CREATE TABLE retained(id INTEGER PRIMARY KEY,epoch INTEGER);
            INSERT INTO head VALUES(1); INSERT INTO retained VALUES(1,1);""")
        canonical = bytes(8)
        data = expected[0][0]
        db.execute("INSERT INTO versions VALUES(?,?,?)", (canonical, 1, data))
        self.assertEqual(baseline.verify(db, work, expected), baseline.digest(*expected, 1))
        for key in (bytes(1), bytes(9)):
            with self.subTest(key_length=len(key)):
                db.execute("INSERT INTO versions VALUES(?,?,?)", (key, 1, data))
                with self.assertRaisesRegex(AssertionError, "noncanonical binary key"):
                    baseline.verify(db, work, expected)
                db.execute("DELETE FROM versions WHERE k=?", (key,))
        db.execute("UPDATE versions SET payload=?", (bytes(16),))
        with self.assertRaisesRegex(AssertionError, "current state mismatch"):
            baseline.verify(db, work, expected)
        db.execute("UPDATE versions SET payload=?", (data,))
        db.execute("UPDATE retained SET epoch=0")
        with self.assertRaisesRegex(AssertionError, "retained catalog mismatch"):
            baseline.verify(db, work, expected)


if __name__ == "__main__":
    unittest.main()
