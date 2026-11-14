"""Compound oracle and process/output supervision controls; no native workload claim."""
import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("compound", Path(__file__).with_name("compound.py"))
runner = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runner)
SOURCE = "a" * 40


def rows(profile):
    result = []
    phases = runner.NATIVE if profile == "native" else runner.FAULTS if profile == "faults" else ["fresh_process_reopen"]
    for phase in phases:
        changed = profile == "faults"
        seq = 0 if profile == "native" and phase in runner.NATIVE[:4] else 4
        survives = phase in runner.FAULTS[4:]
        revision = runner.FINAL + 1 + int(survives) if changed else runner.FIXTURE if seq == 0 else runner.FINAL
        detail = {}
        if phase == "drafts_ready": detail = {"drafts": 8, "prepare_ns": [1] * 8}
        if phase == "core_limit_rejected": detail = {"error": "Limit", "drafts": 8}
        if phase == "drafts_released": detail = {"baseline_live": runner.CORE // 2}
        if phase == "contended_commits": detail = {"writers": 8, "rounds": 4, "commit_ns": [1] * 32, "reader_ns": [1] * 256}
        if phase == "image_limit_rejected": detail = {"error": "Limit", "new_payload_bytes": 4 * runner.MIB}
        if phase == "backup_budget_rejected": detail = {"error": "Limit", "copy_budget": runner.CORE}
        if phase in ["backup", "restore", "compact"]: detail = {"copy_budget": 256 * runner.MIB}
        if changed:
            error = "OutOfMemory" if phase == "allocation" else "Io" if phase in runner.FAULTS[1:3] else "Ok" if phase == "acknowledged" else "OutcomeUnknown"
            detail = {"reopened_revision": runner.FINAL + int(survives), "pressure_live_bytes": runner.CORE * 9 // 10 + 1, "pending_drafts": 2, "error": error}
        result.append({"phase": phase, "revision": revision, "seq": seq, "changed": changed,
                       "roots": 64, "digests": runner.digests(seq, changed), "live_bytes": runner.CORE - 512 if phase == "core_limit_rejected" else runner.CORE // 2,
                       "peak_bytes": runner.CORE, "cap_bytes": runner.CORE, "rss_kib": 1, "file_bytes": 63 * runner.MIB, "elapsed_ns": 1, "detail": detail})
    result.append({"complete": True, "profile": profile, "source_id": SOURCE, "features": 4, "abi": 1, "format": 2})
    return result


class CompoundControls(unittest.TestCase):
    def test_independent_fixed_digests_and_full_profile_schema(self):
        self.assertEqual(runner.fnv([b"hello"]), 0xa430d84680aabd0b)
        self.assertEqual(runner.fixed_digests(), (2395918955545059960, 10593364300825519773, 13346994205906133797))
        self.assertEqual(runner.digests(0, False)[2], 6765992199539664772)
        for profile in ["native", "inspect", "faults"]: runner.check_rows(rows(profile), profile, SOURCE)

    def test_reject_missing_phase_bytes_cap_revision_overlap_and_samples(self):
        for profile in ["native", "faults", "inspect"]:
            valid = rows(profile)
            invalid = [valid[:-1], valid[1:], [*valid, valid[-1]]]
            for field, value in [("digests", [0] * 4), ("revision", 0), ("roots", 63),
                                 ("peak_bytes", runner.CORE + 1), ("file_bytes", 1), ("elapsed_ns", True)]:
                bad = copy.deepcopy(valid); bad[0][field] = value; invalid.append(bad)
            for field, value in [("source_id", "wrong"), ("complete", 1), ("abi", True)]:
                bad = copy.deepcopy(valid); bad[-1][field] = value; invalid.append(bad)
            if profile == "native":
                for index, field, value in [(1, "prepare_ns", [1] * 7), (4, "commit_ns", [1] * 31),
                                             (4, "reader_ns", [1] * 255), (3, "baseline_live", 1)]:
                    bad = copy.deepcopy(valid); bad[index]["detail"][field] = value; invalid.append(bad)
                bad = copy.deepcopy(valid); bad[2]["live_bytes"] = runner.CORE // 2; invalid.append(bad)
                bad = copy.deepcopy(valid); bad[2]["file_bytes"] += 4096; invalid.append(bad)
            if profile == "faults":
                for field, value in [("reopened_revision", 0), ("pressure_live_bytes", 1), ("pending_drafts", 1), ("error", "Ok")]:
                    bad = copy.deepcopy(valid); bad[0]["detail"][field] = value; invalid.append(bad)
            for bad in invalid:
                with self.assertRaises(AssertionError): runner.check_rows(bad, profile, SOURCE)

    def script(self, root, body):
        path = root / "driver.py"; path.write_text(f"#!{sys.executable}\n" + body); path.chmod(0o700); return path

    def test_observer_arguments_and_leader_exit_retire_live_descendant(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            script = self.script(root, "import subprocess,sys,time\n"
                                 "subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)'])\n"
                                 "time.sleep(0.1)\nprint('{\"argument\":\"'+sys.argv[2]+'\"}')\n")
            called = []
            killpg = runner.BASE.os.killpg
            def retire_owned_group(group, signal):
                # A live/unreaped child PID still belongs to this supervisor.
                # The completed leader must not be reaped before group cleanup.
                status = runner.BASE.os.waitid(runner.BASE.os.P_PID, group,
                    runner.BASE.os.WEXITED | runner.BASE.os.WNOHANG | runner.BASE.os.WNOWAIT)
                self.assertIsNotNone(status)
                return killpg(group, signal)
            with patch.object(runner.BASE.os, "killpg", side_effect=retire_owned_group):
                evidence = runner.BASE.observe(script, root / "data", root / "report.json",
                                               checker=lambda records: called.append(records), arguments=["sentinel"])
            self.assertEqual(called, [[{"argument": "sentinel"}]])
            self.assertTrue(evidence["process_group_killed"])
            for _ in range(40):
                live = [p for p in runner.PROCESSES.process_table().values() if p["group"] == evidence["process_group"] and not p["state"].startswith("Z")]
                if not live: break
                time.sleep(0.05)
            self.assertFalse(live)

    def test_observer_output_bound_records_failure_before_parse(self):
        # The helper registry applies an inherited 8 MiB RLIMIT_FSIZE. A
        # truncated producer reaches exactly the bound, which must reject
        # before JSON parsing just like a producer exceeding it without RLIMIT.
        for size in [8, 9]:
            with self.subTest(mib=size), tempfile.TemporaryDirectory() as temp:
                root = Path(temp); script = self.script(root, f"import sys\nsys.stdout.write('x'*({size}*1024*1024))\n")
                with self.assertRaises(AssertionError): runner.BASE.observe(script, root / "data", root / "report.json", checker=lambda records: None)
                report = json.loads((root / "report.json").read_text())
                self.assertFalse(report["passed"]); self.assertIn("output limit", report["failure"])

    def test_fresh_output_rejects_existing_and_dangling(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp); existing = root / "exists"; existing.mkdir(); dangling = root / "link"; dangling.symlink_to(root / "missing")
            for path in [existing, dangling]:
                with self.assertRaises(ValueError): runner.BASE.HELPERS.fresh_output(path)


if __name__ == "__main__": unittest.main()
