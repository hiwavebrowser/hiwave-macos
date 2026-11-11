#!/usr/bin/env python3
"""Stress category report: median, min-max and N per suite, and A vs B.

    python3 tools/stress/report.py [RUNS_DIR ...]
    python3 tools/stress/report.py --compare develop candidate [RUNS_DIR ...]

Reads the run records run.py writes (default ~/zg-measure/stress-runs). Runs
are grouped by suite, engine and label (run.py --label; by default the first
8 of the binary's sha1). Partial runs (--only, --iterations) are left out
unless --partial is given, and are never mixed with full ones. A run that
started at a 1-minute load of 3 or more is discarded (the `disc` column).

--compare BASE CAND applies the landing rule to every suite and engine both
labels have scores for: the candidate regresses when its median is below the
base's median by more than max(FLOOR %, the base's own min-max spread %).
FLOOR defaults to 3 (--floor). Higher is better for all three suites. Exit
code 1 when any suite regresses, or when the base scored and the candidate
did not.

This report is its own: it is not part of the chrome board, RS1, WPT or the
parity campaign.
"""
import argparse
import json
import pathlib
import statistics
import sys

SCORE_LOAD = 3.0
DEFAULT_DIR = pathlib.Path.home() / "zg-measure" / "stress-runs"


def load(dirs, partial):
    runs = []
    for d in dirs:
        for f in sorted(pathlib.Path(d).glob("*.json")):
            try:
                r = json.loads(f.read_text())
            except ValueError:
                continue
            if r.get("schema") != 1 or bool(r.get("partial")) != partial:
                continue
            r["_file"] = f.name
            # The Mac's load rule: a run that started at a 1-minute load of
            # SCORE_LOAD or more keeps its record but not its score.
            r["_valid"] = r["load_avg_before"][0] < SCORE_LOAD
            runs.append(r)
    return runs


def groups(runs):
    out = {}
    for r in runs:
        out.setdefault((r["suite"], r["engine"], r["label"]), []).append(r)
    return out


def summary(rs):
    discarded = len([r for r in rs if not r["_valid"]])
    rs = [r for r in rs if r["_valid"]]
    if not rs:
        return {"n": 0, "n_ok": 0, "not_ok": [], "shas": [], "pins": [], "wall_s": None, "load": None, "discarded": discarded}
    ok = [r["score"] for r in rs if r["status"] == "ok" and isinstance(r.get("score"), (int, float))]
    s = {"discarded": discarded, "n": len(rs), "n_ok": len(ok), "not_ok": sorted({r["status"] for r in rs if r["status"] != "ok"}),
         "shas": sorted({r["binary_sha1"][:10] for r in rs}), "pins": sorted({r["suite_sha"][:10] for r in rs}),
         "wall_s": round(statistics.median(r["wall_s"] for r in rs), 1),
         "load": round(max(r["load_avg_before"][0] for r in rs), 1)}
    if ok:
        med = statistics.median(ok)
        s.update(median=med, min=min(ok), max=max(ok), spread_pct=(max(ok) - min(ok)) / med * 100 if med else 0.0)
    return s


def fmt(v):
    return "-" if v is None else ("%.2f" % v if abs(v) < 1000 else "%.0f" % v)


def sub_medians(rs):
    """Median of each sub-score over the runs that have it."""
    vals = {}
    for r in rs:
        if r["status"] != "ok" or not r["_valid"]:
            continue
        for k, v in (r.get("sub_scores") or {}).items():
            if isinstance(v, (int, float)):
                vals.setdefault(k, []).append(v)
    return {k: statistics.median(v) for k, v in vals.items()}


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("dirs", nargs="*", default=[str(DEFAULT_DIR)])
    ap.add_argument("--compare", nargs=2, metavar=("BASE", "CAND"), help="two run.py labels")
    ap.add_argument("--floor", type=float, default=3.0, help="regression floor, percent (default 3)")
    ap.add_argument("--partial", action="store_true", help="report the partial runs instead of the full ones")
    ap.add_argument("--subs", action="store_true", help="also print per-subtest medians")
    ap.add_argument("--json", action="store_true", help="print the summary as JSON")
    args = ap.parse_args()

    g = groups(load(args.dirs, args.partial))
    if not g:
        print("no %sruns under %s" % ("partial " if args.partial else "", ", ".join(args.dirs)))
        return 2
    table = {k: summary(v) for k, v in g.items()}

    if args.json and not args.compare:
        print(json.dumps([dict(suite=k[0], engine=k[1], label=k[2], **v) for k, v in sorted(table.items())], indent=1))
        return 0

    if not args.compare:
        print("%-12s %-16s %-14s %5s %10s %21s %8s %7s %5s %4s  %s"
              % ("suite", "engine", "label", "N ok", "median", "min - max", "spread", "wall s", "load", "disc", "not ok"))
        for (suite, engine, label), s in sorted(table.items()):
            rng = "%s - %s" % (fmt(s.get("min")), fmt(s.get("max"))) if "median" in s else "-"
            print("%-12s %-16s %-14s %2d/%-2d %10s %21s %7s%% %7s %5s %4d  %s"
                  % (suite, engine, label[:14], s["n_ok"], s["n"], fmt(s.get("median")), rng,
                     "%.1f" % s["spread_pct"] if "median" in s else "-", s["wall_s"], s["load"], s["discarded"],
                     ",".join(s["not_ok"])))
            if args.subs:
                for name, v in sorted(sub_medians(g[(suite, engine, label)]).items()):
                    print("    %-48s %s" % (name, fmt(v)))
        print("disc = runs left out because they started at a 1-minute load of %.0f or more" % SCORE_LOAD)
        return 0

    base, cand = args.compare
    rows, bad = [], False
    for suite, engine in sorted({(k[0], k[1]) for k in table}):
        b, c = table.get((suite, engine, base)), table.get((suite, engine, cand))
        if not b and not c:
            continue
        row = {"suite": suite, "engine": engine, "base": b, "cand": c}
        if not b or "median" not in b:
            row["verdict"] = "NO BASE SCORE"
        elif not c or "median" not in c:
            row["verdict"] = "REGRESSION (base scored, candidate did not)"
            bad = True
        else:
            row["delta_pct"] = (c["median"] - b["median"]) / b["median"] * 100
            row["threshold_pct"] = max(args.floor, b["spread_pct"])
            if row["delta_pct"] < -row["threshold_pct"]:
                row["verdict"] = "REGRESSION"
                bad = True
            elif row["delta_pct"] > row["threshold_pct"]:
                row["verdict"] = "improved"
            else:
                row["verdict"] = "within noise"
            if c["n_ok"] < c["n"] or b["n_ok"] < b["n"]:
                row["verdict"] += " (some runs did not finish)"
        rows.append(row)
    if args.json:
        print(json.dumps(rows, indent=1))
        return 1 if bad else 0
    print("base = %s, candidate = %s; regression when the candidate's median is lower by more than "
          "max(%.1f%%, base min-max spread)" % (base, cand, args.floor))
    print("%-12s %-16s %16s %16s %8s %10s  %s" % ("suite", "engine", "base med (N)", "cand med (N)", "delta", "threshold", "verdict"))
    for r in rows:
        b, c = r["base"] or {}, r["cand"] or {}
        print("%-12s %-16s %11s (%d/%d) %11s (%d/%d) %8s %10s  %s"
              % (r["suite"], r["engine"], fmt(b.get("median")), b.get("n_ok", 0), b.get("n", 0),
                 fmt(c.get("median")), c.get("n_ok", 0), c.get("n", 0),
                 "%+.1f%%" % r["delta_pct"] if "delta_pct" in r else "-",
                 "%.1f%%" % r["threshold_pct"] if "threshold_pct" in r else "-", r["verdict"]))
        if args.subs and "delta_pct" in r:
            sb, sc = sub_medians(g[(r["suite"], r["engine"], base)]), sub_medians(g[(r["suite"], r["engine"], cand)])
            for name in sorted(set(sb) & set(sc)):
                if sb[name]:
                    print("    %-48s %10s %10s %+7.1f%%" % (name, fmt(sb[name]), fmt(sc[name]), (sc[name] - sb[name]) / sb[name] * 100))
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
