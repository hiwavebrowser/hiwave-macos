#!/usr/bin/env python3
"""Chrome-vs-Chrome geometry census: which AXES on this seat are attributable?

NOT A RECEIPT, and no RustKit capture is involved at all. This compares the
seat control (`tools/parity_oracle/capture_seat_control.mjs`) against the pinned
macOS baseline and asks one question per axis: do the two CHROMES agree inside
Gate A's tolerance? Where they do not, a Gate A delta taken on this seat cannot
be attributed to RustKit by magnitude, because the platform is already moving
that axis by more than the bar.

`scripts/seat_control_report.py` answers the same question per FAILING axis,
with RustKit in the picture. This answers it for the whole corpus up front, so a
night can see what it is allowed to work before it picks a unit — night 67 used
it to rule `settings`' `y` column (186 of 189 elements seat-confounded) out and
its `x` column (8 of 189) in.

Diagnostic only, no mutation-checked guards — do not wire into CI. It aggregates
nothing but the two committed/captured rect sets and cannot produce an N/26.

    node tools/parity_oracle/capture_seat_control.mjs --out <dir>
    python3 trench/tools/n67_confound_census.py <dir>
"""
import json
import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts"))
from layout_oracle_gate import (  # noqa: E402
    GEOMETRY_TOLERANCE_PX,
    NON_GATING_SCOPES,
    load_case_registry,
)

AXES = ("x", "y", "width", "height")


def rects(path):
    doc = json.load(open(path))
    return {e["selector"]: e["rect"] for e in doc["elements"]}


def main():
    control = pathlib.Path(sys.argv[1])
    reg = load_case_registry()
    print(f"seat-control confound census — NOT A RECEIPT. tolerance {GEOMETRY_TOLERANCE_PX}px")
    head = f"{'case':24}{'join':>6}{'axes':>6}" + "".join(f"{a:>7}" for a in AXES) + f"{'clean':>7}"
    print(head)
    totals = dict.fromkeys(AXES, 0)
    tot_join = tot_axes = tot_clean = 0
    for cid, case in sorted(reg.items()):
        if case.get("scope") in NON_GATING_SCOPES:
            continue
        pinned = REPO / "baselines/chrome-148" / case["scope"] / cid / "layout-rects.json"
        seat = control / case["scope"] / cid / "layout-rects.json"
        if not pinned.exists() or not seat.exists():
            print(f"{cid:24}  UNMEASURED (no control capture)")
            continue
        P, S = rects(pinned), rects(seat)
        per = dict.fromkeys(AXES, 0)
        joined = clean = 0
        for sel in set(P) & set(S):
            joined += 1
            bad = False
            for axis in AXES:
                if abs(P[sel][axis] - S[sel][axis]) > GEOMETRY_TOLERANCE_PX:
                    per[axis] += 1
                    bad = True
            if not bad:
                clean += 1
        axes = sum(per.values())
        print(
            f"{cid:24}{joined:6}{axes:6}"
            + "".join(f"{per[a]:7}" for a in AXES)
            + f"{clean:7}"
        )
        for a in AXES:
            totals[a] += per[a]
        tot_join += joined
        tot_axes += axes
        tot_clean += clean
    print(
        f"{'TOTAL':24}{tot_join:6}{tot_axes:6}"
        + "".join(f"{totals[a]:7}" for a in AXES)
        + f"{tot_clean:7}"
    )
    print(
        f"\n{tot_clean} of {tot_join} elements agree between the two Chromes on all four axes."
    )


if __name__ == "__main__":
    main()
