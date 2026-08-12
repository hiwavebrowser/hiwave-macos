"""The ratchet must fail on regression, hold on equal red, and never gate blind.

Every rule in ratchet_gate.compare gets a probe in BOTH directions where the
rule has two sides (the paint variance band). The exit-code split is the
load-bearing contract: 1 = regression (the only workflow-failing code),
2 = absolute red without regression, 0 = clean or RATCHET OFF.
"""
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

RATCHET = Path(__file__).resolve().parents[1] / "ratchet_gate.py"


def gate_a(cases):
    return {"gate": "A", "cases": [
        {"case_id": cid, "measured": m.get("measured", True),
         "green": m.get("green", not m.get("geo", [])),
         "geometry_failures": [{"selector": s} for s in m.get("geo", [])],
         "join_failures": [{"selector": s} for s in m.get("join", [])]}
        for cid, m in cases.items()]}


def gate_b(cases):
    return {"gate": "B", "cases": [
        {"case_id": cid, "measured": m.get("measured", True),
         "green": m.get("green", not m.get("disc", []) and m.get("pct", 1.0) >= 0.99),
         "pass_fraction": m.get("pct", 1.0),
         "discrete": [{"kind": k, "selector": s} for k, s in m.get("disc", [])]}
        for cid, m in cases.items()]}


def run(tmp, a, b, baseline=None, extra=None):
    ap, bp = Path(tmp) / "a.json", Path(tmp) / "b.json"
    ap.write_text(json.dumps(a))
    bp.write_text(json.dumps(b))
    cmd = [sys.executable, str(RATCHET), "--gate-a", str(ap), "--gate-b", str(bp)]
    blp = Path(tmp) / "ratchet.json"
    if baseline is not None:
        blp.write_text(json.dumps(baseline))
    cmd += ["--baseline", str(blp)]
    cmd += extra or []
    p = subprocess.run(cmd, capture_output=True, text=True)
    return p.returncode, p.stdout + p.stderr


def seed_from(tmp, a, b):
    rc, _ = run(tmp, a, b, baseline=None,
                extra=["--write-seed", str(Path(tmp) / "ratchet.json")])
    assert rc == 0
    return json.loads((Path(tmp) / "ratchet.json").read_text())


BASE_A = {"clean": {}, "red": {"geo": ["s1", "s2"], "green": False}}
BASE_B = {"clean": {}, "red": {"pct": 0.8, "disc": [("missing_clip", "s1")],
                               "green": False}}


class RatchetTests(unittest.TestCase):
    def _seeded(self, tmp):
        return seed_from(tmp, gate_a(BASE_A), gate_b(BASE_B))

    def test_hold_equal_red_exits_2(self):
        with tempfile.TemporaryDirectory() as t:
            rc, out = run(t, gate_a(BASE_A), gate_b(BASE_B), self._seeded(t))
            self.assertEqual(rc, 2, out)
            self.assertIn("none worse", out)

    def test_geometry_count_up_exits_1(self):
        with tempfile.TemporaryDirectory() as t:
            a = {"clean": {}, "red": {"geo": ["s1", "s2", "s3"], "green": False}}
            rc, out = run(t, gate_a(a), gate_b(BASE_B), self._seeded(t))
            self.assertEqual(rc, 1, out)
            self.assertIn("geometry failures 2 -> 3", out)

    def test_join_count_up_exits_1(self):
        with tempfile.TemporaryDirectory() as t:
            a = {"clean": {"join": ["j1"]}, "red": BASE_A["red"]}
            rc, out = run(t, gate_a(a), gate_b(BASE_B), self._seeded(t))
            self.assertEqual(rc, 1, out)
            self.assertIn("join failures", out)

    def test_new_discrete_id_exits_1(self):
        with tempfile.TemporaryDirectory() as t:
            b = {"clean": {}, "red": {"pct": 0.8, "green": False,
                                      "disc": [("missing_clip", "s1"),
                                               ("wrong_solid_color", "s9")]}}
            rc, out = run(t, gate_a(BASE_A), gate_b(b), self._seeded(t))
            self.assertEqual(rc, 1, out)
            self.assertIn("NEW discrete failure wrong_solid_color::s9", out)

    def test_paint_below_band_exits_1_within_band_holds(self):
        with tempfile.TemporaryDirectory() as t:
            seeded = self._seeded(t)
            below = {"clean": {}, "red": {"pct": 0.6, "green": False,
                                          "disc": [("missing_clip", "s1")]}}
            rc, out = run(t, gate_a(BASE_A), gate_b(below), seeded)
            self.assertEqual(rc, 1, out)
        with tempfile.TemporaryDirectory() as t:
            seeded = self._seeded(t)
            within = {"clean": {}, "red": {"pct": 0.79, "green": False,
                                           "disc": [("missing_clip", "s1")]}}
            rc, out = run(t, gate_a(BASE_A), gate_b(within), seeded)
            self.assertEqual(rc, 2, out)  # 0.01 drop < 0.1 band: not a regression

    def test_green_to_red_flip_exits_1(self):
        with tempfile.TemporaryDirectory() as t:
            a = {"clean": {"geo": [], "green": False}, "red": BASE_A["red"]}
            rc, out = run(t, gate_a(a), gate_b(BASE_B), self._seeded(t))
            self.assertEqual(rc, 1, out)
            self.assertIn("geometry_green flipped green -> red", out)

    def test_unmeasured_when_baseline_measured_exits_1(self):
        with tempfile.TemporaryDirectory() as t:
            a = {"clean": {}, "red": {"measured": False, "green": False}}
            rc, out = run(t, gate_a(a), gate_b(BASE_B), self._seeded(t))
            self.assertEqual(rc, 1, out)
            self.assertIn("UNMEASURED now", out)

    def test_improvement_reports_tighten_and_does_not_fail(self):
        with tempfile.TemporaryDirectory() as t:
            a = {"clean": {}, "red": {"geo": ["s1"], "green": False}}
            rc, out = run(t, gate_a(a), gate_b(BASE_B), self._seeded(t))
            self.assertEqual(rc, 2, out)
            self.assertIn("tighten-eligible", out)
            self.assertIn("red", out)

    def test_no_baseline_is_ratchet_off_exit_0(self):
        with tempfile.TemporaryDirectory() as t:
            rc, out = run(t, gate_a(BASE_A), gate_b(BASE_B), baseline=None)
            self.assertEqual(rc, 0, out)
            self.assertIn("RATCHET OFF", out)

    def test_missing_gate_report_fails_loud(self):
        with tempfile.TemporaryDirectory() as t:
            blp = Path(t) / "ratchet.json"
            blp.write_text(json.dumps({"schema": 1, "cases": {}}))
            p = subprocess.run(
                [sys.executable, str(RATCHET), "--gate-a", str(Path(t) / "nope.json"),
                 "--gate-b", str(Path(t) / "b.json"), "--baseline", str(blp)],
                capture_output=True, text=True)
            self.assertEqual(p.returncode, 1, p.stdout)
            self.assertIn("did not run", p.stdout)

    def test_seed_round_trip_holds(self):
        with tempfile.TemporaryDirectory() as t:
            rc, out = run(t, gate_a(BASE_A), gate_b(BASE_B), self._seeded(t))
            self.assertIn(rc, (0, 2), out)
            self.assertNotIn("REGRESSION", out)


if __name__ == "__main__":
    unittest.main()
