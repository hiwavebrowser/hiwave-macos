#!/usr/bin/env python3
"""Compare two parity_test.py result JSONs case by case (diffPixels).

    receipt_diff.py <reference.json> <candidate.json>
"""
import json
import sys


def load(path):
    out = {}
    for r in json.load(open(path))["results"]:
        out[r["case_id"]] = (r.get("type"), (r.get("pixel") or {}).get("diffPixels"),
                             (r.get("pixel") or {}).get("diffPercent"))
    return out


ref, cand = load(sys.argv[1]), load(sys.argv[2])
differ = [(k, ref.get(k, (None, None))[1], v[1]) for k, v in cand.items() if ref.get(k, (None, None))[1] != v[1]]
print("%d cases in candidate, %d in reference; diffPixels differ on %d: %s" % (len(cand), len(ref), len(differ), differ))
print("builtins:", ", ".join("%s %.2f%%" % (k, v[2]) for k, v in cand.items() if v[0] == "builtins"))
