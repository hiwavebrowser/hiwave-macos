"""Regression check: errored parity cases score as worst-case, never pass.

Guards the CI/local discrepancy fixed in trench night-2: parity_test.py
scored errored captures as 100 while extract_parity_metrics.py crashed on
pixel=None (or would have defaulted a missing diffPercent to 0.0 — a pass).
Run: python3 scripts/test_extract_metrics_errored.py
"""
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from extract_parity_metrics import extract_metrics

# An errored case exactly as run_test() emits it (pixel=None, error set)
errored = {
    "case_id": "settings",
    "type": "builtins",
    "threshold": 15,
    "pixel": None,
    "styles": None,
    "rects": None,
    "passed": False,
    "error": "Capture failed: Timeout",
    "diff_pct": 100.0,
}
# A legacy errored case from before this fix (no diff_pct at all)
legacy_errored = dict(errored, case_id="legacy", diff_pct=None)
del legacy_errored["diff_pct"]
# A healthy passing case
healthy = {
    "case_id": "about",
    "type": "builtins",
    "threshold": 15,
    "pixel": {"diffPercent": 3.2},
    "passed": True,
}

m = extract_metrics({"results": [errored, legacy_errored, healthy]})
by_name = {t["name"]: t for t in m["tests"]}

assert by_name["settings"]["diff"] == 100.0, by_name["settings"]
assert by_name["settings"]["passed"] is False
assert by_name["settings"]["error"] == "Capture failed: Timeout"
assert by_name["legacy"]["diff"] == 100.0, by_name["legacy"]
assert by_name["legacy"]["passed"] is False
assert by_name["about"]["diff"] == 3.2
assert by_name["about"]["passed"] is True
assert m["passed"] == 1 and m["failed"] == 2
assert m["worst_case"]["diff"] == 100.0

print("OK: errored cases score 100.0 and fail; healthy case unaffected")
print(json.dumps(m["worst_case"]))
