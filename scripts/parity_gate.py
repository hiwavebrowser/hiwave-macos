#!/usr/bin/env python3
"""
parity_gate.py - CI gate for parity threshold enforcement

This script checks that the parity baseline meets minimum requirements.
Exit codes:
  0 = Pass (parity meets threshold)
  1 = Fail (parity below threshold or errors)

Usage:
    python3 scripts/parity_gate.py [--minimum <pct>] [--report <path>]
    
Examples:
    python3 scripts/parity_gate.py --minimum 80
    python3 scripts/parity_gate.py --report parity-baseline/baseline_report.json
    python3 scripts/parity_gate.py --minimum 80 --fail-on-regression 2
"""

import json
import sys
from pathlib import Path
from typing import Dict, Any, Optional


def load_report(report_path: Path) -> Optional[Dict[str, Any]]:
    """Load a baseline report JSON."""
    if not report_path.exists():
        print(f"Error: Report not found at {report_path}")
        return None
    
    try:
        with open(report_path) as f:
            return json.load(f)
    except json.JSONDecodeError as e:
        print(f"Error: Invalid JSON in {report_path}: {e}")
        return None


def compute_parity(report: Dict[str, Any]) -> float:
    """Compute overall parity percentage from a report."""
    metrics = report.get("metrics", {})
    weighted_mean_diff = metrics.get("tier_b_weighted_mean", 100)
    return 100 - weighted_mean_diff


def check_regressions(current_report: Dict, previous_report: Dict, threshold: float) -> list:
    """Check for case regressions exceeding threshold."""
    regressions = []
    
    current_results = {}
    for r in current_report.get("builtin_results", []) + current_report.get("websuite_results", []):
        current_results[r["case_id"]] = r.get("estimated_diff_pct", 100)
    
    previous_results = {}
    for r in previous_report.get("builtin_results", []) + previous_report.get("websuite_results", []):
        previous_results[r["case_id"]] = r.get("estimated_diff_pct", 100)
    
    for case_id, current_diff in current_results.items():
        previous_diff = previous_results.get(case_id, 100)
        delta = current_diff - previous_diff
        
        if delta > threshold:
            regressions.append({
                "case_id": case_id,
                "previous_diff": previous_diff,
                "current_diff": current_diff,
                "delta": delta,
            })
    
    return regressions


def main():
    # Default values
    minimum_parity = 80.0
    report_path = Path("parity-baseline/baseline_report.json")
    previous_path = None
    regression_threshold = None
    verbose = False
    
    # Parse arguments
    args = sys.argv[1:]
    i = 0
    while i < len(args):
        if args[i] == "--minimum" and i + 1 < len(args):
            minimum_parity = float(args[i + 1])
            i += 2
        elif args[i] == "--report" and i + 1 < len(args):
            report_path = Path(args[i + 1])
            i += 2
        elif args[i] == "--previous" and i + 1 < len(args):
            previous_path = Path(args[i + 1])
            i += 2
        elif args[i] == "--fail-on-regression" and i + 1 < len(args):
            regression_threshold = float(args[i + 1])
            i += 2
        elif args[i] in ["-v", "--verbose"]:
            verbose = True
            i += 1
        elif args[i] in ["-h", "--help"]:
            print(__doc__)
            sys.exit(0)
        else:
            i += 1
    
    print("=" * 60)
    print("Parity Gate Check")
    print("=" * 60)
    print(f"\nMinimum Required: {minimum_parity}%")
    print(f"Report: {report_path}")
    if regression_threshold:
        print(f"Regression Threshold: {regression_threshold}%")
    
    # Load current report
    report = load_report(report_path)
    if not report:
        sys.exit(1)
    
    # Compute parity
    parity = compute_parity(report)
    metrics = report.get("metrics", {})
    
    print(f"\n--- Results ---")
    print(f"Current Parity: {parity:.1f}%")
    print(f"Tier A Pass Rate: {metrics.get('tier_a_pass_rate', 0) * 100:.1f}%")
    print(f"Weighted Mean Diff: {metrics.get('tier_b_weighted_mean', 100):.1f}%")
    
    # Check minimum parity
    parity_passed = parity >= minimum_parity
    
    if parity_passed:
        print(f"\n✓ PASS: Parity {parity:.1f}% >= {minimum_parity}% minimum")
    else:
        print(f"\n✗ FAIL: Parity {parity:.1f}% < {minimum_parity}% minimum")
    
    # Check regressions if previous report provided
    regressions = []
    if regression_threshold and previous_path:
        previous_report = load_report(previous_path)
        if previous_report:
            regressions = check_regressions(report, previous_report, regression_threshold)
            
            if regressions:
                print(f"\n✗ FAIL: {len(regressions)} case(s) regressed by >{regression_threshold}%:")
                for r in sorted(regressions, key=lambda x: -x["delta"]):
                    print(f"  - {r['case_id']}: {r['previous_diff']:.1f}% -> {r['current_diff']:.1f}% (+{r['delta']:.1f}%)")
            else:
                print(f"\n✓ PASS: No regressions exceeding {regression_threshold}%")
    
    # Verbose output
    if verbose:
        print("\n--- Per-Case Results ---")
        all_results = report.get("builtin_results", []) + report.get("websuite_results", [])
        for r in sorted(all_results, key=lambda x: x.get("estimated_diff_pct", 100), reverse=True):
            case_id = r["case_id"]
            diff = r.get("estimated_diff_pct", 100)
            status = "✓" if diff <= 25 else "✗"
            source = r.get("diff_source", "heuristic")
            print(f"  {status} {case_id}: {diff:.1f}% ({source})")
    
    # Final verdict
    print("\n" + "=" * 60)
    
    if parity_passed and not regressions:
        print("GATE: PASSED")
        sys.exit(0)
    else:
        print("GATE: FAILED")
        sys.exit(1)


if __name__ == "__main__":
    main()

