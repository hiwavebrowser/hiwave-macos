#!/usr/bin/env python3
"""
parity_swarm.py - Parallel/sharded swarm parity testing

This script enables:
- Parallel execution across multiple processes
- CI sharding for distributed testing
- Scout-then-Exploit scheduling for maximum ROI
- Run-scoped artifacts (safe for parallel execution)

Usage:
    # Local parallel run (4 workers)
    python3 scripts/parity_swarm.py --jobs 4

    # CI shard 0 of 4
    python3 scripts/parity_swarm.py --shard-index 0 --shard-count 4

    # Scout then exploit top 5 worst cases
    python3 scripts/parity_swarm.py --jobs 4 --exploit-top 5 --exploit-iterations 3

    # Time-budgeted run
    python3 scripts/parity_swarm.py --jobs 4 --budget-minutes 30
"""

import argparse
import json
import multiprocessing as mp
import sys
import time
from collections import defaultdict
from dataclasses import dataclass, asdict
from datetime import datetime
from pathlib import Path
from typing import List, Dict, Any, Optional, Tuple

from parity_lib import (
    REPO_ROOT,
    DEFAULT_RESULTS_ROOT,
    BUILTINS,
    WEBSUITE,
    MICRO_TESTS,
    VIEWPORTS,
    WorkUnit,
    CaseResult,
    AggregatedResult,
    get_threshold,
    get_case_type,
    get_all_cases,
    generate_run_id,
    execute_work_unit,
    aggregate_iterations,
    ensure_parity_capture_built,
    result_to_dict,
    aggregated_to_dict,
)


# ============================================================================
# Work unit generation
# ============================================================================

def generate_work_units(
    scope: str = "all",
    cases: Optional[List[str]] = None,
    viewports: Optional[List[str]] = None,
    iterations: int = 1,
) -> List[WorkUnit]:
    """
    Generate deterministic ordered list of work units.
    
    Order is stable for reproducible sharding:
    - Sorted by (case_id, viewport, iteration)
    """
    all_cases = get_all_cases()
    
    # Filter cases by scope or explicit list
    if cases:
        case_list = [(cid, *all_cases[cid][1:]) for cid in cases if cid in all_cases]
    else:
        case_list = []
        if scope in ["all", "builtins"]:
            case_list.extend([(c[0], c[1], c[2], c[3], "builtins") for c in BUILTINS])
        if scope in ["all", "websuite"]:
            case_list.extend([(c[0], c[1], c[2], c[3], "websuite") for c in WEBSUITE])
        if scope in ["all", "micro"]:
            case_list.extend([(c[0], c[1], c[2], c[3], "micro") for c in MICRO_TESTS])
    
    # Filter viewports
    if viewports:
        vp_set = set(viewports)
        viewport_list = [(w, h, name) for w, h, name in VIEWPORTS if name in vp_set]
    else:
        # Use case's native viewport (width x height from case definition)
        viewport_list = None
    
    # Generate work units
    units = []
    for case_id, html_path, case_w, case_h, case_type in sorted(case_list, key=lambda x: x[0]):
        if viewport_list:
            for vp_w, vp_h, vp_name in viewport_list:
                for iter_idx in range(iterations):
                    units.append(WorkUnit(
                        case_id=case_id,
                        html_path=html_path,
                        width=vp_w,
                        height=vp_h,
                        case_type=case_type,
                        viewport_name=vp_name,
                        iteration=iter_idx + 1,
                    ))
        else:
            # Use native viewport
            vp_name = f"{case_w}x{case_h}"
            for iter_idx in range(iterations):
                units.append(WorkUnit(
                    case_id=case_id,
                    html_path=html_path,
                    width=case_w,
                    height=case_h,
                    case_type=case_type,
                    viewport_name=vp_name,
                    iteration=iter_idx + 1,
                ))
    
    return units


def shard_work_units(
    units: List[WorkUnit],
    shard_index: int,
    shard_count: int,
) -> List[WorkUnit]:
    """
    Deterministically shard work units by modulo.
    
    shard 0/4 gets units 0, 4, 8, ...
    shard 1/4 gets units 1, 5, 9, ...
    """
    return [u for i, u in enumerate(units) if i % shard_count == shard_index]


# ============================================================================
# Worker process
# ============================================================================

def worker_execute(args: Tuple[WorkUnit, str, Path]) -> CaseResult:
    """Worker function for multiprocessing pool."""
    work_unit, run_id, results_root = args
    return execute_work_unit(work_unit, run_id, results_root)


# ============================================================================
# Scout-Exploit scheduler
# ============================================================================

@dataclass
class SwarmConfig:
    """Configuration for swarm run."""
    run_id: str
    results_root: Path
    jobs: int
    scope: str
    cases: Optional[List[str]]
    viewports: Optional[List[str]]
    iterations: int
    exploit_top: int
    exploit_iterations: int
    exploit_viewports: List[str]
    budget_minutes: Optional[int]
    max_variance: float
    dry_run: bool


def run_scout_phase(
    config: SwarmConfig,
    units: List[WorkUnit],
) -> Tuple[List[CaseResult], float]:
    """
    Scout phase: 1 iteration, 1 viewport per case.
    Returns results and elapsed seconds.
    """
    print(f"\n{'='*60}")
    print("SCOUT PHASE: Quick scan of all cases")
    print(f"{'='*60}")
    print(f"Work units: {len(units)}")
    print(f"Workers: {config.jobs}")
    
    if config.dry_run:
        print("[DRY RUN] Would execute:")
        for u in units[:10]:
            print(f"  - {u.case_id} @ {u.viewport_name} iter {u.iteration}")
        if len(units) > 10:
            print(f"  ... and {len(units) - 10} more")
        return [], 0.0
    
    start = time.time()
    
    # Prepare args for pool
    args = [(u, config.run_id, config.results_root) for u in units]
    
    results: List[CaseResult] = []
    
    if config.jobs == 1:
        # Sequential execution
        for i, arg in enumerate(args):
            result = worker_execute(arg)
            results.append(result)
            status = "✓" if result.passed else "✗" if not result.error else "E"
            print(f"  [{i+1}/{len(args)}] {result.case_id}: {status} {result.diff_pct:.2f}%")
    else:
        # Parallel execution
        with mp.Pool(processes=config.jobs) as pool:
            for i, result in enumerate(pool.imap_unordered(worker_execute, args)):
                results.append(result)
                status = "✓" if result.passed else "✗" if not result.error else "E"
                print(f"  [{i+1}/{len(args)}] {result.case_id}: {status} {result.diff_pct:.2f}%")
    
    elapsed = time.time() - start
    
    # Summary
    passed = sum(1 for r in results if r.passed)
    errors = sum(1 for r in results if r.error)
    avg_diff = sum(r.diff_pct for r in results if not r.error) / max(1, len(results) - errors)
    
    print(f"\nScout complete: {passed}/{len(results)} passed, {errors} errors")
    print(f"Average diff: {avg_diff:.2f}%")
    print(f"Elapsed: {elapsed:.1f}s")
    
    return results, elapsed


def run_exploit_phase(
    config: SwarmConfig,
    scout_results: List[CaseResult],
    time_remaining: Optional[float],
) -> Tuple[List[CaseResult], float]:
    """
    Exploit phase: More iterations and viewports on worst cases.
    Returns additional results and elapsed seconds.
    """
    # Rank cases by diff (worst first)
    case_diffs: Dict[str, float] = {}
    case_info: Dict[str, Tuple[str, str]] = {}  # case_id -> (html_path, case_type)
    
    for r in scout_results:
        if not r.error:
            existing = case_diffs.get(r.case_id, float('inf'))
            if r.diff_pct > existing or r.case_id not in case_diffs:
                case_diffs[r.case_id] = r.diff_pct
    
    # Get case info
    all_cases = get_all_cases()
    for case_id in case_diffs:
        if case_id in all_cases:
            _, html, w, h, ctype = all_cases[case_id]
            case_info[case_id] = (html, ctype)
    
    # Select top K worst
    sorted_cases = sorted(case_diffs.items(), key=lambda x: -x[1])
    exploit_cases = sorted_cases[:config.exploit_top]
    
    if not exploit_cases:
        print("\nNo cases to exploit.")
        return [], 0.0
    
    print(f"\n{'='*60}")
    print(f"EXPLOIT PHASE: Deep dive on top {len(exploit_cases)} worst cases")
    print(f"{'='*60}")
    print("Target cases:")
    for case_id, diff in exploit_cases:
        print(f"  - {case_id}: {diff:.2f}%")
    
    # Generate exploit work units
    exploit_units = []
    for case_id, _ in exploit_cases:
        if case_id not in case_info:
            continue
        html_path, case_type = case_info[case_id]
        
        # Multiple viewports
        for vp_name in config.exploit_viewports:
            vp = next((v for v in VIEWPORTS if v[2] == vp_name), None)
            if not vp:
                continue
            vp_w, vp_h, _ = vp
            
            # Multiple iterations
            for iter_idx in range(config.exploit_iterations):
                exploit_units.append(WorkUnit(
                    case_id=case_id,
                    html_path=html_path,
                    width=vp_w,
                    height=vp_h,
                    case_type=case_type,
                    viewport_name=vp_name,
                    iteration=iter_idx + 1,
                ))
    
    print(f"Exploit work units: {len(exploit_units)}")
    print(f"Viewports: {config.exploit_viewports}")
    print(f"Iterations per viewport: {config.exploit_iterations}")
    
    if config.dry_run:
        print("[DRY RUN] Would execute exploit units")
        return [], 0.0
    
    start = time.time()
    
    # Prepare args for pool
    args = [(u, config.run_id, config.results_root) for u in exploit_units]
    
    results: List[CaseResult] = []
    
    if config.jobs == 1:
        for i, arg in enumerate(args):
            result = worker_execute(arg)
            results.append(result)
            status = "✓" if result.passed else "✗" if not result.error else "E"
            print(f"  [{i+1}/{len(args)}] {result.case_id}@{result.viewport}: {status} {result.diff_pct:.2f}%")
    else:
        with mp.Pool(processes=config.jobs) as pool:
            for i, result in enumerate(pool.imap_unordered(worker_execute, args)):
                results.append(result)
                status = "✓" if result.passed else "✗" if not result.error else "E"
                print(f"  [{i+1}/{len(args)}] {result.case_id}@{result.viewport}: {status} {result.diff_pct:.2f}%")
    
    elapsed = time.time() - start
    print(f"\nExploit complete. Elapsed: {elapsed:.1f}s")
    
    return results, elapsed


# ============================================================================
# Result aggregation and output
# ============================================================================

def aggregate_swarm_results(
    scout_results: List[CaseResult],
    exploit_results: List[CaseResult],
    config: SwarmConfig,
) -> Dict[str, Any]:
    """
    Aggregate all results into final report.
    
    Groups by (case_id, viewport) and computes stats.
    """
    all_results = scout_results + exploit_results
    
    # Group by (case_id, viewport)
    groups: Dict[Tuple[str, str], List[CaseResult]] = defaultdict(list)
    for r in all_results:
        key = (r.case_id, r.viewport)
        groups[key].append(r)
    
    # Aggregate each group
    aggregated: List[Dict[str, Any]] = []
    for (case_id, viewport), results in groups.items():
        agg = aggregate_iterations(results, config.max_variance)
        aggregated.append(aggregated_to_dict(agg))
    
    # Sort by diff descending
    aggregated.sort(key=lambda x: -x["diff_pct_median"])
    
    # Global stats
    all_diffs = [a["diff_pct_median"] for a in aggregated]
    passed = sum(1 for a in aggregated if a["passed"])
    stable = sum(1 for a in aggregated if a.get("stable", False))
    
    # Global taxonomy (sum contributions across cases)
    global_taxonomy: Dict[str, float] = defaultdict(float)
    for a in aggregated:
        if a.get("best_taxonomy"):
            for bucket, pct in a["best_taxonomy"].items():
                global_taxonomy[bucket] += pct
    
    # Normalize taxonomy
    total_tax = sum(global_taxonomy.values())
    if total_tax > 0:
        global_taxonomy = {k: (v / total_tax) * 100 for k, v in global_taxonomy.items()}
    
    return {
        "run_id": config.run_id,
        "timestamp": datetime.now().isoformat(),
        "config": {
            "scope": config.scope,
            "jobs": config.jobs,
            "scout_iterations": config.iterations,
            "exploit_top": config.exploit_top,
            "exploit_iterations": config.exploit_iterations,
            "exploit_viewports": config.exploit_viewports,
            "max_variance": config.max_variance,
        },
        "summary": {
            "total_cases": len(aggregated),
            "passed": passed,
            "failed": len(aggregated) - passed,
            "stable": stable,
            "avg_diff_pct": sum(all_diffs) / len(all_diffs) if all_diffs else 100,
            "min_diff_pct": min(all_diffs) if all_diffs else 100,
            "max_diff_pct": max(all_diffs) if all_diffs else 100,
        },
        "global_taxonomy": dict(sorted(global_taxonomy.items(), key=lambda x: -x[1])),
        "results": aggregated,
        "raw_results": {
            "scout": [result_to_dict(r) for r in scout_results],
            "exploit": [result_to_dict(r) for r in exploit_results],
        },
    }


def save_results(report: Dict[str, Any], config: SwarmConfig) -> Path:
    """Save final report to run directory."""
    run_dir = config.results_root / config.run_id
    run_dir.mkdir(parents=True, exist_ok=True)
    
    report_path = run_dir / "swarm_report.json"
    with open(report_path, "w") as f:
        json.dump(report, f, indent=2)
    
    # Also save a summary for quick viewing
    summary_path = run_dir / "summary.txt"
    with open(summary_path, "w") as f:
        s = report["summary"]
        f.write(f"Parity Swarm Report: {config.run_id}\n")
        f.write(f"{'='*50}\n\n")
        f.write(f"Passed: {s['passed']}/{s['total_cases']}\n")
        f.write(f"Average Diff: {s['avg_diff_pct']:.2f}%\n")
        f.write(f"Stable: {s['stable']}\n\n")
        f.write("Top Taxonomy Buckets:\n")
        for bucket, pct in list(report["global_taxonomy"].items())[:5]:
            f.write(f"  {bucket}: {pct:.1f}%\n")
        f.write("\nWorst Cases:\n")
        for r in report["results"][:10]:
            f.write(f"  {r['case_id']}@{r['viewport']}: {r['diff_pct_median']:.2f}%\n")
    
    return report_path


# ============================================================================
# Main
# ============================================================================

def main():
    parser = argparse.ArgumentParser(
        description="Swarm parity testing with parallel/sharded execution",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    
    # Execution mode
    parser.add_argument("--jobs", "-j", type=int, default=1,
                        help="Number of parallel workers (default: 1)")
    parser.add_argument("--shard-index", type=int, default=None,
                        help="Shard index for CI distribution (0-based)")
    parser.add_argument("--shard-count", type=int, default=None,
                        help="Total number of shards")
    
    # Scope
    parser.add_argument("--scope", type=str, default="all",
                        choices=["all", "builtins", "websuite", "micro"],
                        help="Test scope (default: all)")
    parser.add_argument("--cases", type=str, default=None,
                        help="Comma-separated list of specific cases")
    parser.add_argument("--viewports", type=str, default=None,
                        help="Comma-separated viewports (e.g., 800x600,1280x800)")
    
    # Scout phase
    parser.add_argument("--iterations", type=int, default=1,
                        help="Scout iterations per case (default: 1)")
    
    # Exploit phase
    parser.add_argument("--exploit-top", type=int, default=0,
                        help="Exploit top N worst cases after scout (default: 0 = no exploit)")
    parser.add_argument("--exploit-iterations", type=int, default=3,
                        help="Iterations per exploit case (default: 3)")
    parser.add_argument("--exploit-viewports", type=str, default="800x600,1280x800,1920x1080",
                        help="Viewports for exploit phase")
    
    # Budget
    parser.add_argument("--budget-minutes", type=int, default=None,
                        help="Time budget in minutes (best effort)")
    
    # Stability
    parser.add_argument("--max-variance", type=float, default=0.10,
                        help="Max variance for stability (default: 0.10)")
    
    # Output
    parser.add_argument("--output-dir", type=str, default=None,
                        help="Results root directory")
    parser.add_argument("--run-id", type=str, default=None,
                        help="Custom run ID (default: auto-generated)")
    
    # Other
    parser.add_argument("--dry-run", action="store_true",
                        help="Print planned work without executing")
    parser.add_argument("--skip-build", action="store_true",
                        help="Skip building parity-capture")
    
    args = parser.parse_args()
    
    # Validate sharding
    if (args.shard_index is None) != (args.shard_count is None):
        parser.error("--shard-index and --shard-count must be used together")
    
    # Build config
    run_id = args.run_id or generate_run_id()
    results_root = Path(args.output_dir) if args.output_dir else DEFAULT_RESULTS_ROOT
    
    config = SwarmConfig(
        run_id=run_id,
        results_root=results_root,
        jobs=args.jobs,
        scope=args.scope,
        cases=args.cases.split(",") if args.cases else None,
        viewports=args.viewports.split(",") if args.viewports else None,
        iterations=args.iterations,
        exploit_top=args.exploit_top,
        exploit_iterations=args.exploit_iterations,
        exploit_viewports=args.exploit_viewports.split(","),
        budget_minutes=args.budget_minutes,
        max_variance=args.max_variance,
        dry_run=args.dry_run,
    )
    
    print("=" * 60)
    print("PARITY SWARM")
    print("=" * 60)
    print(f"Run ID: {config.run_id}")
    print(f"Results: {config.results_root / config.run_id}")
    print(f"Workers: {config.jobs}")
    print(f"Scope: {config.scope}")
    if args.shard_index is not None:
        print(f"Shard: {args.shard_index + 1}/{args.shard_count}")
    print(f"Timestamp: {datetime.now().isoformat()}")
    
    # Build parity-capture
    if not args.dry_run and not args.skip_build:
        print("\nBuilding parity-capture...")
        if not ensure_parity_capture_built():
            print("ERROR: Failed to build parity-capture")
            sys.exit(1)
        print("Build complete.")
    
    # Generate work units
    all_units = generate_work_units(
        scope=config.scope,
        cases=config.cases,
        viewports=config.viewports,
        iterations=config.iterations,
    )
    
    # Apply sharding if specified
    if args.shard_index is not None:
        all_units = shard_work_units(all_units, args.shard_index, args.shard_count)
        print(f"\nShard {args.shard_index + 1}/{args.shard_count}: {len(all_units)} units")
    
    if not all_units:
        print("\nNo work units to execute.")
        sys.exit(0)
    
    # Run scout phase
    start_time = time.time()
    scout_results, scout_elapsed = run_scout_phase(config, all_units)
    
    # Run exploit phase if configured
    exploit_results: List[CaseResult] = []
    if config.exploit_top > 0 and scout_results:
        time_remaining = None
        if config.budget_minutes:
            time_remaining = (config.budget_minutes * 60) - scout_elapsed
            if time_remaining < 60:
                print(f"\nSkipping exploit phase: only {time_remaining:.0f}s remaining")
            else:
                exploit_results, _ = run_exploit_phase(config, scout_results, time_remaining)
        else:
            exploit_results, _ = run_exploit_phase(config, scout_results, None)
    
    total_elapsed = time.time() - start_time
    
    # Aggregate and save
    if not config.dry_run:
        report = aggregate_swarm_results(scout_results, exploit_results, config)
        report["elapsed_seconds"] = total_elapsed
        report_path = save_results(report, config)
        
        # Final summary
        print("\n" + "=" * 60)
        print("FINAL SUMMARY")
        print("=" * 60)
        s = report["summary"]
        print(f"Total cases: {s['total_cases']}")
        print(f"Passed: {s['passed']}/{s['total_cases']} ({100*s['passed']/max(1,s['total_cases']):.1f}%)")
        print(f"Average diff: {s['avg_diff_pct']:.2f}%")
        print(f"Stable: {s['stable']}")
        print(f"\nTotal elapsed: {total_elapsed:.1f}s ({total_elapsed/60:.1f}m)")
        print(f"\nReport saved to: {report_path}")
        
        # Show worst cases
        print("\nWorst 5 cases:")
        for r in report["results"][:5]:
            stable_str = " (stable)" if r.get("stable") else ""
            print(f"  {r['case_id']}@{r['viewport']}: {r['diff_pct_median']:.2f}%{stable_str}")
        
        # Show taxonomy
        if report["global_taxonomy"]:
            print("\nGlobal taxonomy:")
            for bucket, pct in list(report["global_taxonomy"].items())[:5]:
                print(f"  {bucket}: {pct:.1f}%")
        
        # Exit code based on pass rate
        if s["passed"] < s["total_cases"]:
            sys.exit(1)
    
    sys.exit(0)


if __name__ == "__main__":
    # Required for multiprocessing on macOS
    mp.set_start_method("spawn", force=True)
    main()
