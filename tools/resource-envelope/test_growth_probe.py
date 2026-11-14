"""Strict disposable instrumentation and measurement-parser controls."""
import copy
import importlib.util
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("growth_probe", Path(__file__).with_name("growth_probe.py"))
probe = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(probe)


class GrowthProbeControls(unittest.TestCase):
    def test_unique_instrumentation_and_strict_domain_measurements(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); (root / "src").mkdir()
            path = root / "src/memory.rs"
            path.write_text((probe.COMPOUND.ROOT / "src/memory.rs").read_text())
            probe.instrument(root)
            text = path.read_text()
            self.assertEqual(text.count("VEC_GROWS.fetch_add("), 1)
            self.assertEqual(text.count("impl Drop for MemoryState"), 1)
            with self.assertRaises(AssertionError): probe.instrument(root)
            for text in ["no matching site", probe.NEEDLE + probe.NEEDLE]:
                path.write_text(text)
                with self.assertRaises(AssertionError): probe.instrument(root)
        valid = "GROWTH_DOMAIN cap=10 peak=9 used=0 grows=2 old=4 new=9\n"
        self.assertEqual(probe.parse(valid)[0]["old"], 4)
        for text in ["", valid + "GROWTH_DOMAIN broken\n", valid.replace("used=0", "used=1"),
                     valid.replace("peak=9", "peak=11"), valid + valid.replace("grows=2", "grows=1")]:
            with self.assertRaises(AssertionError): probe.parse(text)


        # Reuse the compound oracle's synthetic complete profiles. A verified
        # flag alone must not allow zip() to accept truncated clean evidence.
        spec = importlib.util.spec_from_file_location("growth_fixture", Path(__file__).with_name("test_compound.py"))
        fixture = importlib.util.module_from_spec(spec); spec.loader.exec_module(fixture)
        hashes = {"src/memory.rs": "sentinel"}
        reference = {"verified": True, "revision": fixture.SOURCE, "source_hashes": hashes,
                     "runs": {p: {"passed": True, "owned_live_processes_absent": True, "rows": fixture.rows(p)}
                              for p in ["native", "inspect", "faults"]}}
        probe.validate_reference(reference, fixture.SOURCE, hashes)
        invalid = []
        for key, value in [("verified", 1), ("revision", "wrong"), ("source_hashes", {})]:
            bad = copy.deepcopy(reference); bad[key] = value; invalid.append(bad)
        bad = copy.deepcopy(reference); del bad["runs"]["faults"]; invalid.append(bad)
        bad = copy.deepcopy(reference); bad["runs"]["native"]["rows"] = []; invalid.append(bad)
        bad = copy.deepcopy(reference); bad["runs"]["native"]["passed"] = False; invalid.append(bad)
        bad = copy.deepcopy(reference); bad["runs"]["inspect"]["owned_live_processes_absent"] = False; invalid.append(bad)
        for bad in invalid:
            with self.assertRaises(AssertionError): probe.validate_reference(bad, fixture.SOURCE, hashes)


if __name__ == "__main__": unittest.main()
