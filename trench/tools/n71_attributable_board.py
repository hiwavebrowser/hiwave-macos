#!/usr/bin/env python3
"""Rank Gate A's geometry failures by whether this seat is ALLOWED to work them.

NOT A RECEIPT. This produces no N/26 and no parity number. It reads two Gate A
runs over the SAME RustKit captures and differs only in which Chrome they are
scored against:

    pinned  = baselines/chrome-148      -> Delta_reported = RustKit_seat - Chrome_macos
    control = baselines/seat-control    -> Delta_real     = RustKit_seat - Chrome_seat

A failure present in BOTH is one this seat can attribute to RustKit's box math:
the seat's own Chrome, on the seat's own fonts, disagrees with RustKit too. A
failure present only in the pinned run is dominated by the platform confound and
must not be picked as a unit here — night 67 ruled `settings`' y column out this
way (186 of 189 elements seat-confounded).

The reverse class matters as well and is reported: present only in the CONTROL
run. Those are boxes where RustKit happens to match macOS Chrome while
disagreeing with the seat's Chrome, i.e. the confound is cancelling a defect.
They are not units either, and counting them as green would be the Goodhart
move this campaign exists to end.

    python3 trench/tools/n71_attributable_board.py <pinned gate-a.json> <control gate-a.json>
"""
import collections
import json
import re
import sys

AXES = ("x", "y", "width", "height")


def load(path):
    doc = json.load(open(path))
    out = {}
    for case in doc["cases"]:
        for f in case.get("failures", []):
            if f.get("axis") not in AXES:
                continue  # join failures and non-geometry kinds
            out[(case["case_id"], f["path"], f["axis"])] = f
    return doc, out


def family(selector):
    """Collapse a selector to its defect family: tag + first class, nth stripped."""
    if not selector:
        return "(anonymous)"
    s = re.sub(r":nth-child\(\d+\)", "", selector)
    s = s.strip().split()[-1] if s.strip() else selector
    return s


def main():
    pinned_doc, pinned = load(sys.argv[1])
    control_doc, control = load(sys.argv[2])

    both = pinned.keys() & control.keys()
    pinned_only = pinned.keys() - control.keys()
    control_only = control.keys() - pinned.keys()

    print("n71 attributable board — NOT A RECEIPT, no N/26 here")
    print(f"  pinned  gate-a: {len(pinned)} geometry failures")
    print(f"  control gate-a: {len(control)} geometry failures")
    print(f"  ATTRIBUTABLE (fails against both Chromes): {len(both)}")
    print(f"  confound-dominated (pinned only):          {len(pinned_only)}")
    print(f"  confound-MASKED  (control only):           {len(control_only)}")

    print("\nattributable failures per case and axis")
    per = collections.defaultdict(collections.Counter)
    for cid, path, axis in both:
        per[cid][axis] += 1
    head = f"{'case':24}" + "".join(f"{a:>8}" for a in AXES) + f"{'total':>8}"
    print(head)
    rows = sorted(per.items(), key=lambda kv: -sum(kv[1].values()))
    for cid, c in rows:
        print(f"{cid:24}" + "".join(f"{c[a]:>8}" for a in AXES) + f"{sum(c.values()):>8}")

    print("\nattributable failures by selector family (top 25)")
    fam = collections.Counter()
    famdelta = collections.defaultdict(float)
    for key in both:
        f = control[key]
        k = (key[0], family(f.get("selector")), key[2])
        fam[k] += 1
        famdelta[k] += abs(f["delta"] or 0.0)
    print(f"{'case':20}{'family':34}{'axis':>7}{'n':>5}{'sum|d|':>11}{'mean|d|':>9}")
    for (cid, famname, axis), n in fam.most_common(25):
        s = famdelta[(cid, famname, axis)]
        print(f"{cid:20}{famname[:33]:34}{axis:>7}{n:>5}{s:>11.3f}{s / n:>9.3f}")


if __name__ == "__main__":
    main()
