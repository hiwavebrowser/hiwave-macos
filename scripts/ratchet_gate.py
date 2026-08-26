#!/usr/bin/env python3
"""Ratchet regression gate over Gate A / Gate B / stability.

Blocking teeth without pretending absolute green (design pin, Prometheus
2026-08-12): a case FAILS this gate only when it regresses against the
committed baseline — absolute red that matches the baseline is visible but
does not fail. The baseline tightens by MANUAL commit only (a tiny PR that
touches nothing but the baseline file); this script never writes it during
a gated run.

Exit codes (the workflow step must fail ONLY on 1):
  0  no regression, nothing absolutely red beyond baseline — or RATCHET OFF
     (no baseline committed), stated loudly rather than inferred
  1  REGRESSION — at least one case is worse than the committed floor
  2  absolute red exists but nothing regressed (the honest steady state)
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

BASELINE_DEFAULT = Path("trench/ratchet-gates-baseline.json")
SCHEMA = 1


def _case_map(report: Optional[Dict[str, Any]]) -> Dict[str, Dict[str, Any]]:
    if not report:
        return {}
    return {c.get("case_id"): c for c in report.get("cases", []) if c.get("case_id")}


def discrete_ids(case: Dict[str, Any]) -> List[str]:
    """Stable identity for a discrete failure: kind::selector.

    Production Gate B rows carry discrete failures inside `failures[]`,
    flagged `"discrete": true` (only attributable ones — the geometry
    precondition already ran inside the gate). The percentage-bar entry in
    the same list carries `"discrete": false` and is not an id.
    """
    out = []
    for f in case.get("failures", []) or []:
        if f.get("discrete"):
            out.append(f"{f.get('kind', '?')}::{f.get('selector', '?')}")
    return sorted(out)


def snapshot(
    gate_a: Optional[Dict[str, Any]],
    gate_b: Optional[Dict[str, Any]],
    max_variance: float,
) -> Dict[str, Any]:
    """One ratchet row per case present in either gate report."""
    amap, bmap = _case_map(gate_a), _case_map(gate_b)
    rows: Dict[str, Any] = {}
    for cid in sorted(set(amap) | set(bmap)):
        a, b = amap.get(cid), bmap.get(cid)
        # Production schema (pinned by test_production_schema_excerpt):
        # geometry_failures / join_failures are COUNTS, not lists; the
        # per-case paint fraction is `within_fraction`.
        rows[cid] = {
            "geometry_measured": bool(a and a.get("measured")),
            "geometry_green": bool(a and a.get("green")),
            "geometry_fail_count": int((a or {}).get("geometry_failures") or 0),
            "join_fail_count": int((a or {}).get("join_failures") or 0),
            "paint_measured": bool(b and b.get("measured")),
            "paint_green": bool(b and b.get("green")),
            "paint_pct": float((b or {}).get("within_fraction") or 0.0),
            "discrete_fail_ids": discrete_ids(b or {}),
        }
    return {"schema": SCHEMA, "max_variance": max_variance, "cases": rows}


def compare(
    baseline: Dict[str, Any], current: Dict[str, Any]
) -> Tuple[List[str], List[str], List[str]]:
    """-> (regressions, absolute_reds, tighten_eligible), each human lines."""
    regressions: List[str] = []
    absolute: List[str] = []
    tighten: List[str] = []
    variance = float(baseline.get("max_variance", 0.1))
    base_cases: Dict[str, Any] = baseline.get("cases", {})

    for cid, base in sorted(base_cases.items()):
        cur = current["cases"].get(cid)
        if cur is None or (
            (base.get("geometry_measured") and not cur["geometry_measured"])
            or (base.get("paint_measured") and not cur["paint_measured"])
        ):
            regressions.append(
                f"{cid}: measured in baseline, UNMEASURED now — instrument regression"
            )
            continue

        if cur["geometry_fail_count"] > base.get("geometry_fail_count", 0):
            regressions.append(
                f"{cid}: geometry failures {base.get('geometry_fail_count', 0)}"
                f" -> {cur['geometry_fail_count']}"
            )
        if cur["join_fail_count"] > base.get("join_fail_count", 0):
            regressions.append(
                f"{cid}: join failures {base.get('join_fail_count', 0)}"
                f" -> {cur['join_fail_count']}"
            )
        new_ids = sorted(
            set(cur["discrete_fail_ids"]) - set(base.get("discrete_fail_ids", []))
        )
        for nid in new_ids:
            regressions.append(f"{cid}: NEW discrete failure {nid}")
        floor = base.get("paint_pct", 0.0) - variance
        if cur["paint_pct"] < floor:
            regressions.append(
                f"{cid}: paint {base.get('paint_pct', 0.0):.5f}"
                f" -> {cur['paint_pct']:.5f} (below variance band {variance})"
            )
        for col in ("geometry_green", "paint_green"):
            if base.get(col) and not cur[col]:
                regressions.append(f"{cid}: {col} flipped green -> red")

        red = (not cur["geometry_green"]) or (not cur["paint_green"]) or cur[
            "discrete_fail_ids"
        ]
        if red:
            absolute.append(
                f"{cid}: geo_fails={cur['geometry_fail_count']}"
                f" paint={cur['paint_pct']:.5f} discrete={len(cur['discrete_fail_ids'])}"
            )
        improved = (
            cur["geometry_fail_count"] < base.get("geometry_fail_count", 0)
            or cur["paint_pct"] > base.get("paint_pct", 0.0) + variance
            or set(cur["discrete_fail_ids"]) < set(base.get("discrete_fail_ids", []))
        )
        if improved:
            tighten.append(cid)

    return regressions, absolute, tighten


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--gate-a", type=Path, required=True)
    ap.add_argument("--gate-b", type=Path, required=True)
    ap.add_argument("--baseline", type=Path, default=BASELINE_DEFAULT)
    ap.add_argument(
        "--max-variance",
        type=float,
        default=0.1,
        help="paint %% band reused from the stability bar — do not invent a second",
    )
    ap.add_argument("--engine-sha", help="engine commit the captures were produced from")
    ap.add_argument("--receipt-run", help="CI run id/url the gate reports came from")
    ap.add_argument("--stability-runs", type=int,
                    help="iterations behind the captures (seed law: >= 3)")
    ap.add_argument(
        "--write-seed",
        type=Path,
        help="write a baseline snapshot from the current reports and exit 0 "
        "(seeding is a separate, manual, baseline-only commit)",
    )
    args = ap.parse_args()

    def load(p: Path) -> Optional[Dict[str, Any]]:
        return json.load(open(p)) if p.exists() else None

    gate_a, gate_b = load(args.gate_a), load(args.gate_b)
    for name, rep, p in (("A", gate_a, args.gate_a), ("B", gate_b, args.gate_b)):
        if rep is None:
            print(f"RATCHET: Gate {name} report missing at {p} — it did not run.")
            print("A gate that did not run cannot be ratcheted. Failing loud.")
            return 1

    current = snapshot(gate_a, gate_b, args.max_variance)

    if args.write_seed:
        # Provenance travels WITH the floor (configs are receipts): a reader
        # of the committed baseline must not need archaeology to learn which
        # engine, which run, and how many iterations stand behind it.
        import datetime
        current["provenance"] = {
            "engine_sha": args.engine_sha,
            "receipt_run": args.receipt_run,
            "stability_runs": args.stability_runs,
            "captured_at": datetime.datetime.now(datetime.timezone.utc)
            .isoformat(timespec="seconds"),
        }
        missing = [k for k, v in current["provenance"].items() if v in (None, "")]
        if missing:
            print(f"RATCHET seed REFUSED — provenance missing: {', '.join(missing)}. "
                  "A floor without provenance is a number without a receipt.")
            return 1
        if args.stability_runs is not None and args.stability_runs < 3:
            print(f"RATCHET seed REFUSED — stability_runs={args.stability_runs} < 3 "
                  "(seed law: honest macOS receipt, N>=3).")
            return 1
        args.write_seed.parent.mkdir(parents=True, exist_ok=True)
        with open(args.write_seed, "w") as fh:
            json.dump(current, fh, indent=2, sort_keys=True)
        print(f"RATCHET: seed written to {args.write_seed} "
              f"({len(current['cases'])} cases). Commit it in a baseline-only PR.")
        return 0

    if not args.baseline.exists():
        print(
            f"RATCHET OFF — no baseline committed at {args.baseline}. "
            "This run measured everything and gated nothing; seed with "
            "--write-seed from an honest macOS receipt and commit it."
        )
        return 0

    baseline = json.load(open(args.baseline))
    regressions, absolute, tighten = compare(baseline, current)

    if tighten:
        print(f"RATCHET tighten-eligible ({len(tighten)}): {', '.join(tighten)}")
        print("  (manual baseline-only PR — this script never writes during a run)")
    if regressions:
        print(f"RATCHET REGRESSION ({len(regressions)}):")
        for r in regressions:
            print(f"  {r}")
        return 1
    if absolute:
        print(f"RATCHET holds: {len(absolute)} case(s) absolutely red, none worse "
              "than the committed floor:")
        for a in absolute:
            print(f"  {a}")
        return 2
    print("RATCHET holds: no regression, no absolute red beyond baseline.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
