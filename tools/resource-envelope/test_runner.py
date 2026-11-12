"""No-overwrite, cleanup and oracle controls. Optional frozen evidence via env."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("resource_envelope", Path(__file__).with_name("run.py"))
runner = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runner)


class RunnerTests(unittest.TestCase):
    def script(self, root, body):
        script = root / "driver.py"
        script.write_text(f"#!{sys.executable}\n" + body)
        script.chmod(0o700)
        return script

    def test_observer_existing_outputs_remain_untouched(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            data = root / "data"
            data.mkdir()
            with self.assertRaises(ValueError):
                runner.observe(root / "absent", data, root / "run.json")
            self.assertFalse((root / "run.json").exists())
            report = root / "existing.json"
            report.write_bytes(b"preserve")
            with self.assertRaises(ValueError):
                runner.observe(root / "absent", root / "new", report)
            self.assertEqual(report.read_bytes(), b"preserve")

    def test_malformed_output_retains_terminal_failure(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            script = self.script(root, "print('deliberately invalid JSON')\n")
            report = root / "run.json"
            with self.assertRaises(AssertionError):
                runner.observe(script, root / "data", report)
            evidence = json.loads(report.read_text())
            self.assertFalse(evidence["passed"])
            self.assertEqual(evidence["returncode"], 0)
            self.assertIn("JSONDecodeError", evidence["failure"])

    def test_timeout_kills_worker_process_group_and_records_failure(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            pidfile = root / "grandchild.pid"
            script = self.script(root, "import subprocess,sys,time\n"
                "from pathlib import Path\n"
                "child=subprocess.Popen([sys.executable,'-c','import time;time.sleep(30)'])\n"
                f"Path({str(pidfile)!r}).write_text(str(child.pid))\n"
                "time.sleep(30)\n")
            report = root / "run.json"
            with self.assertRaises(AssertionError):
                runner.observe(script, root / "data", report, deadline_seconds=0.3)
            evidence = json.loads(report.read_text())
            self.assertFalse(evidence["passed"])
            self.assertTrue(evidence["process_group_killed"])
            self.assertIn("TimeoutError", evidence["failure"])
            with self.assertRaises(ProcessLookupError):
                os.kill(evidence["process_group"], 0)
            self.assertTrue(pidfile.exists(), "control must actually spawn a descendant")
            pid = int(pidfile.read_text())
            # Orphan zombies may await init reaping; no live descendant may run.
            for _ in range(20):
                state = subprocess.run(["ps", "-o", "stat=", "-p", str(pid)],
                                       capture_output=True, text=True).stdout.strip()
                if not state or state.startswith("Z"):
                    break
                time.sleep(0.05)
            self.assertTrue(not state or state.startswith("Z"), state)

    def test_small_independent_oracle_known_digest(self):
        self.assertEqual(runner.oracle_hash((1, 1), 8192), 11582251074155391637)

    def test_sampler_timeout_reaps_real_child_and_retains_failure(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            script = self.script(root, "import time\ntime.sleep(30)\n")
            report = root / "run.json"
            with patch.object(runner.sys, "platform", "darwin"), patch.object(
                    runner.subprocess, "run", side_effect=subprocess.TimeoutExpired(["ps"], 5)) as sampler:
                with self.assertRaises(AssertionError):
                    runner.observe(script, root / "data", report)
            sampler.assert_called_once()
            self.assertEqual(sampler.call_args.kwargs["timeout"], 5)
            self.assertEqual(sampler.call_args.args[0][0], "ps")
            evidence = json.loads(report.read_text())
            self.assertFalse(evidence["passed"])
            self.assertTrue(evidence["process_group_killed"])
            self.assertIn("TimeoutExpired", evidence["failure"])
            self.assertIsNotNone(evidence["returncode"])
            with self.assertRaises(ProcessLookupError):
                os.kill(evidence["process_group"], 0)

    @unittest.skipUnless(os.environ.get("RESOURCE_ENVELOPE_EVIDENCE"), "frozen evidence not supplied")
    def test_frozen_positive_and_corrupted_oracle_controls(self):
        evidence = json.loads(Path(os.environ["RESOURCE_ENVELOPE_EVIDENCE"]).read_text())
        rows = evidence["run"]["rows"]
        self.assertTrue(evidence["passed"])
        runner.check_rows(rows)
        for field, value in (("oracle_digest", 0), ("current_count", 2),
                             ("live_bytes", 1 << 40), ("retained_count", 63)):
            corrupt = copy.deepcopy(rows)
            corrupt[1][field] = value
            with self.subTest(field=field), self.assertRaises(AssertionError):
                runner.check_rows(corrupt)
        with self.assertRaises(AssertionError):
            runner.check_rows(rows[:-1])


if __name__ == "__main__":
    unittest.main()
