#!/usr/bin/env python3
"""
parity_baseline.py - Capture baseline parity metrics and cluster failures

This script:
1. Runs RustKit capture for all built-ins + websuite cases
2. Computes per-case pixel diff % (simulated if no Chromium baseline available)
3. Computes weighted tiered metrics (built-ins 60%, websuite 40%)
4. Clusters failures into: sizing/layout, paint, text, images
5. Saves a reproducible baseline report
6. Auto-archives to parity-history/ for tracking progress

Usage:
    python3 scripts/parity_baseline.py [--tag <name>] [--output-dir <dir>]
    
Examples:
    python3 scripts/parity_baseline.py
    python3 scripts/parity_baseline.py --tag "after-flex-fix"
    python3 scripts/parity_baseline.py --no-archive
    python3 scripts/parity_baseline.py --gpu  # Requires display/GPU
    
Options:
    --gpu           Use GPU-based capture (requires display). Default is headless.
    --tag <name>    Tag this run with a name for easier identification.
    --output-dir    Output directory for captures and reports.
    --no-archive    Skip auto-archiving to parity-history/.
    --release       Use release build (default: release).
    --debug         Use debug build instead of release.
"""

import json
import os
import shutil
import subprocess
import sys
from datetime import datetime
from pathlib import Path
from typing import Dict, List, Any, Tuple, Optional

# Import archive functions
from parity_archive import archive_run, get_previous_run, extract_summary

# Configuration
BUILTINS_WEIGHT = 0.60
WEBSUITE_WEIGHT = 0.40
TIER_A_THRESHOLD = 25  # Start with 25% diff threshold

# Built-in pages (60% weight)
BUILTINS = [
    ("new_tab", "crates/hiwave-app/src/ui/new_tab.html", 1280, 800),
    ("about", "crates/hiwave-app/src/ui/about.html", 800, 600),
    ("settings", "crates/hiwave-app/src/ui/settings.html", 1024, 768),
    ("chrome_rustkit", "crates/hiwave-app/src/ui/chrome_rustkit.html", 1280, 100),
    ("shelf", "crates/hiwave-app/src/ui/shelf.html", 1280, 120),
]

# Websuite cases (40% weight)
WEBSUITE = [
    ("article-typography", "websuite/cases/article-typography/index.html", 1280, 800),
    ("card-grid", "websuite/cases/card-grid/index.html", 1280, 800),
    ("css-selectors", "websuite/cases/css-selectors/index.html", 800, 1200),
    ("flex-positioning", "websuite/cases/flex-positioning/index.html", 800, 1000),
    ("form-elements", "websuite/cases/form-elements/index.html", 800, 600),
    ("gradient-backgrounds", "websuite/cases/gradient-backgrounds/index.html", 800, 600),
    ("image-gallery", "websuite/cases/image-gallery/index.html", 1280, 800),
    ("sticky-scroll", "websuite/cases/sticky-scroll/index.html", 1280, 800),
]


def run_rustkit_capture(
    case_id: str, 
    html_path: str, 
    width: int, 
    height: int, 
    output_dir: Path,
    use_gpu: bool = False,
    use_release: bool = True,
) -> Dict[str, Any]:
    """Run parity-capture to capture a frame and layout tree.
    
    Args:
        case_id: Identifier for this test case
        html_path: Path to HTML file to render
        width: Viewport width
        height: Viewport height
        output_dir: Directory to save outputs
        use_gpu: If True, use GPU-based capture (requires display). 
                 If False, use headless capture (may fail without GPU adapter).
        use_release: If True, use release build. If False, use debug.
    """
    frame_path = output_dir / f"{case_id}.ppm"
    layout_path = output_dir / f"{case_id}.layout.json"
    
    # Build command based on capture mode
    build_mode = "--release" if use_release else ""
    
    if use_gpu:
        # GPU mode: Use the full hiwave-smoke harness with display
        # This requires a running display but gives accurate GPU rendering
        cmd = [
            "cargo", "run", "-p", "hiwave-smoke",
        ]
        if use_release:
            cmd.insert(2, "--release")
        cmd.extend([
            "--",
            "--html-file", html_path,
            "--width", str(width),
            "--height", str(height),
            "--dump-frame", str(frame_path),
            "--duration-ms", "2000",  # Short duration for testing
        ])
    else:
        # Headless mode: Use parity-capture (may fail without GPU adapter)
        cmd = [
            "cargo", "run", "-p", "parity-capture",
        ]
        if use_release:
            cmd.insert(2, "--release")
        cmd.extend([
            "--",
            "--html-file", html_path,
            "--width", str(width),
            "--height", str(height),
            "--dump-frame", str(frame_path),
            "--dump-layout", str(layout_path),
        ])
    
    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=120,
            cwd=Path(__file__).parent.parent,
        )
        
        # Parse JSON result from stdout (last line)
        capture_result = {}
        for line in result.stdout.strip().split('\n'):
            if line.startswith('{') and '"status"' in line:
                try:
                    capture_result = json.loads(line)
                except json.JSONDecodeError:
                    pass
        
        success = capture_result.get("status") == "ok" and frame_path.exists()
        
        # Determine error message
        error_msg = None
        if not success:
            error_msg = capture_result.get("error")
            if not error_msg:
                # No JSON error, check stderr or exit code
                if result.returncode != 0:
                    # Get last few lines of stderr
                    stderr_lines = result.stderr.strip().split('\n')[-3:]
                    error_msg = '; '.join(line.strip() for line in stderr_lines if line.strip())
                if not error_msg:
                    error_msg = f"Process exited with code {result.returncode}"
            
            # Suggest --gpu flag for GPU errors
            if error_msg and "GPU" in error_msg:
                error_msg += " (Try running with --gpu flag if you have a display)"
        
        return {
            "case_id": case_id,
            "html_path": html_path,
            "width": width,
            "height": height,
            "success": success,
            "frame_path": str(frame_path) if success else None,
            "layout_path": str(layout_path) if layout_path.exists() else None,
            "layout_stats": capture_result.get("layout_stats"),
            "perf": {},
            "error": error_msg,
            "capture_mode": "gpu" if use_gpu else "headless",
        }
    except subprocess.TimeoutExpired:
        return {
            "case_id": case_id,
            "success": False,
            "error": "Timeout after 120s",
        }
    except Exception as e:
        return {
            "case_id": case_id,
            "success": False,
            "error": str(e),
        }


def analyze_layout(layout_path: str) -> Dict[str, Any]:
    """Analyze a layout tree JSON for issues."""
    if not layout_path or not Path(layout_path).exists():
        return {"error": "No layout file"}
    
    with open(layout_path) as f:
        data = json.load(f)
    
    stats = {
        "total_boxes": 0,
        "positioned": 0,
        "sized": 0,
        "zero_size": 0,
        "at_origin": 0,
        "form_controls": 0,
        "text_boxes": 0,
        "block_boxes": 0,
        "image_boxes": 0,
        "issues": [],
    }
    
    def walk(node, depth=0):
        rect = node.get("content_rect") or node.get("rect", {})
        x, y = rect.get("x", 0), rect.get("y", 0)
        w, h = rect.get("width", 0), rect.get("height", 0)
        node_type = node.get("type", "unknown")
        
        stats["total_boxes"] += 1
        
        if x != 0 or y != 0:
            stats["positioned"] += 1
        else:
            stats["at_origin"] += 1
        
        if w > 0 and h > 0:
            stats["sized"] += 1
        else:
            stats["zero_size"] += 1
            if depth > 1 and node_type not in ["text", "anonymous_block"]:
                stats["issues"].append({
                    "type": "zero_size",
                    "node_type": node_type,
                    "depth": depth,
                })
        
        if node_type == "form_control":
            stats["form_controls"] += 1
        elif node_type == "text":
            stats["text_boxes"] += 1
        elif node_type == "block":
            stats["block_boxes"] += 1
        elif node_type == "image":
            stats["image_boxes"] += 1
        
        for child in node.get("children", []):
            walk(child, depth + 1)
    
    if "root" in data:
        walk(data["root"])
    
    # Compute sizing rate
    stats["sizing_rate"] = stats["sized"] / max(1, stats["total_boxes"])
    stats["positioning_rate"] = stats["positioned"] / max(1, stats["total_boxes"])
    
    return stats


def classify_issues(layout_stats: Dict[str, Any]) -> Dict[str, int]:
    """Classify issues into buckets."""
    clusters = {
        "sizing_layout": 0,
        "paint": 0,
        "text": 0,
        "images": 0,
    }
    
    for issue in layout_stats.get("issues", []):
        if issue["type"] == "zero_size":
            if issue["node_type"] in ["form_control", "block"]:
                clusters["sizing_layout"] += 1
            elif issue["node_type"] == "text":
                clusters["text"] += 1
            elif issue["node_type"] == "image":
                clusters["images"] += 1
            else:
                clusters["sizing_layout"] += 1
    
    return clusters


def estimate_diff_percent(layout_stats: Dict[str, Any]) -> float:
    """
    Estimate pixel diff % based on layout analysis.
    
    This is a heuristic until we have actual Chromium baselines.
    Higher sizing_rate and positioning_rate = lower diff.
    """
    if "error" in layout_stats:
        return 100.0
    
    sizing_rate = layout_stats.get("sizing_rate", 0)
    positioning_rate = layout_stats.get("positioning_rate", 0)
    
    # Base diff estimate: inverse of quality
    base_diff = 100 * (1 - (sizing_rate * 0.6 + positioning_rate * 0.4))
    
    # Add penalty for zero-size boxes
    zero_penalty = min(30, layout_stats.get("zero_size", 0) * 2)
    
    # Clamp to 0-100
    return min(100, max(0, base_diff + zero_penalty))


def compute_weighted_metrics(
    builtin_results: List[Dict],
    websuite_results: List[Dict],
) -> Dict[str, Any]:
    """Compute weighted tiered metrics."""
    
    def get_diffs(results):
        return [r.get("estimated_diff_pct", 100) for r in results]
    
    builtin_diffs = get_diffs(builtin_results)
    websuite_diffs = get_diffs(websuite_results)
    
    # Tier A: Pass rate under threshold
    builtin_pass = sum(1 for d in builtin_diffs if d <= TIER_A_THRESHOLD) / max(1, len(builtin_diffs))
    websuite_pass = sum(1 for d in websuite_diffs if d <= TIER_A_THRESHOLD) / max(1, len(websuite_diffs))
    
    weighted_pass_rate = builtin_pass * BUILTINS_WEIGHT + websuite_pass * WEBSUITE_WEIGHT
    
    # Tier B: Median diff
    all_diffs = builtin_diffs + websuite_diffs
    all_diffs.sort()
    median_diff = all_diffs[len(all_diffs) // 2] if all_diffs else 100
    
    # Weighted median (approximate)
    weighted_median = (
        (sum(builtin_diffs) / max(1, len(builtin_diffs))) * BUILTINS_WEIGHT +
        (sum(websuite_diffs) / max(1, len(websuite_diffs))) * WEBSUITE_WEIGHT
    )
    
    # Top 3 worst cases
    all_results = [(r, "builtin") for r in builtin_results] + [(r, "websuite") for r in websuite_results]
    all_results.sort(key=lambda x: x[0].get("estimated_diff_pct", 100), reverse=True)
    worst_3 = [
        {"case_id": r["case_id"], "type": t, "diff_pct": r.get("estimated_diff_pct", 100)}
        for r, t in all_results[:3]
    ]
    
    return {
        "tier_a_threshold": TIER_A_THRESHOLD,
        "tier_a_pass_rate": weighted_pass_rate,
        "tier_a_builtin_pass": builtin_pass,
        "tier_a_websuite_pass": websuite_pass,
        "tier_b_median_diff": median_diff,
        "tier_b_weighted_mean": weighted_median,
        "worst_3_cases": worst_3,
        "builtin_mean_diff": sum(builtin_diffs) / max(1, len(builtin_diffs)),
        "websuite_mean_diff": sum(websuite_diffs) / max(1, len(websuite_diffs)),
    }


def main():
    output_dir = Path("parity-baseline")
    history_dir = Path("parity-history")
    tag = None
    auto_archive = True
    use_gpu = False
    use_release = True
    
    # Parse arguments
    args = sys.argv[1:]
    i = 0
    while i < len(args):
        if args[i] == "--output-dir" and i + 1 < len(args):
            output_dir = Path(args[i + 1])
            i += 2
        elif args[i] == "--tag" and i + 1 < len(args):
            tag = args[i + 1]
            i += 2
        elif args[i] == "--history" and i + 1 < len(args):
            history_dir = Path(args[i + 1])
            i += 2
        elif args[i] == "--no-archive":
            auto_archive = False
            i += 1
        elif args[i] == "--gpu":
            use_gpu = True
            i += 1
        elif args[i] == "--release":
            use_release = True
            i += 1
        elif args[i] == "--debug":
            use_release = False
            i += 1
        elif args[i] in ["-h", "--help"]:
            print(__doc__)
            sys.exit(0)
        else:
            i += 1
    
    output_dir.mkdir(parents=True, exist_ok=True)
    captures_dir = output_dir / "captures"
    captures_dir.mkdir(exist_ok=True)
    
    print("=" * 60)
    print("Parity Baseline Capture")
    print(f"Output: {output_dir}")
    if tag:
        print(f"Tag: {tag}")
    print(f"Capture Mode: {'GPU (requires display)' if use_gpu else 'Headless'}")
    print(f"Build: {'Release' if use_release else 'Debug'}")
    print(f"Timestamp: {datetime.now().isoformat()}")
    print("=" * 60)
    
    # Capture built-ins
    print("\n--- Built-in Pages (60% weight) ---")
    builtin_results = []
    for case_id, html_path, width, height in BUILTINS:
        print(f"  Capturing {case_id}...", end=" ", flush=True)
        result = run_rustkit_capture(case_id, html_path, width, height, captures_dir, use_gpu, use_release)
        
        if result["success"]:
            # Use layout_stats from capture if available, otherwise analyze
            layout_stats = result.get("layout_stats")
            if not layout_stats:
                layout_stats = analyze_layout(result.get("layout_path"))
            result["layout_stats"] = layout_stats
            result["estimated_diff_pct"] = estimate_diff_percent(layout_stats)
            result["issue_clusters"] = classify_issues(layout_stats)
            print(f"OK (sizing: {layout_stats.get('sizing_rate', 0)*100:.1f}%, est. diff: {result['estimated_diff_pct']:.1f}%)")
        else:
            result["estimated_diff_pct"] = 100
            result["issue_clusters"] = {"sizing_layout": 1}
            error_msg = result.get('error') or 'Unknown error'
            print(f"FAIL: {error_msg[:50]}")
        
        builtin_results.append(result)
    
    # Capture websuite
    print("\n--- Websuite Cases (40% weight) ---")
    websuite_results = []
    for case_id, html_path, width, height in WEBSUITE:
        print(f"  Capturing {case_id}...", end=" ", flush=True)
        result = run_rustkit_capture(case_id, html_path, width, height, captures_dir, use_gpu, use_release)
        
        if result["success"]:
            # Use layout_stats from capture if available, otherwise analyze
            layout_stats = result.get("layout_stats")
            if not layout_stats:
                layout_stats = analyze_layout(result.get("layout_path"))
            result["layout_stats"] = layout_stats
            result["estimated_diff_pct"] = estimate_diff_percent(layout_stats)
            result["issue_clusters"] = classify_issues(layout_stats)
            print(f"OK (sizing: {layout_stats.get('sizing_rate', 0)*100:.1f}%, est. diff: {result['estimated_diff_pct']:.1f}%)")
        else:
            result["estimated_diff_pct"] = 100
            result["issue_clusters"] = {"sizing_layout": 1}
            error_msg = result.get('error') or 'Unknown error'
            print(f"FAIL: {error_msg[:50]}")
        
        websuite_results.append(result)
    
    # Compute metrics
    print("\n--- Computing Weighted Tiered Metrics ---")
    metrics = compute_weighted_metrics(builtin_results, websuite_results)
    
    # Aggregate issue clusters
    total_clusters = {"sizing_layout": 0, "paint": 0, "text": 0, "images": 0}
    for r in builtin_results + websuite_results:
        for k, v in r.get("issue_clusters", {}).items():
            total_clusters[k] += v
    
    # Build report
    report = {
        "timestamp": datetime.now().isoformat(),
        "config": {
            "builtins_weight": BUILTINS_WEIGHT,
            "websuite_weight": WEBSUITE_WEIGHT,
            "tier_a_threshold": TIER_A_THRESHOLD,
        },
        "metrics": metrics,
        "issue_clusters": total_clusters,
        "builtin_results": builtin_results,
        "websuite_results": websuite_results,
    }
    
    # Save report
    report_path = output_dir / "baseline_report.json"
    with open(report_path, "w") as f:
        json.dump(report, f, indent=2, default=str)
    
    # Print summary
    print("\n" + "=" * 60)
    print("BASELINE SUMMARY")
    print("=" * 60)
    print(f"\nTier A (Pass Rate @ {TIER_A_THRESHOLD}% threshold):")
    print(f"  Weighted: {metrics['tier_a_pass_rate']*100:.1f}%")
    print(f"  Built-ins: {metrics['tier_a_builtin_pass']*100:.1f}%")
    print(f"  Websuite: {metrics['tier_a_websuite_pass']*100:.1f}%")
    
    print(f"\nTier B (Diff %):")
    print(f"  Median: {metrics['tier_b_median_diff']:.1f}%")
    print(f"  Weighted Mean: {metrics['tier_b_weighted_mean']:.1f}%")
    print(f"  Built-in Mean: {metrics['builtin_mean_diff']:.1f}%")
    print(f"  Websuite Mean: {metrics['websuite_mean_diff']:.1f}%")
    
    print(f"\nWorst 3 Cases:")
    for w in metrics["worst_3_cases"]:
        print(f"  {w['case_id']} ({w['type']}): {w['diff_pct']:.1f}%")
    
    print(f"\nIssue Clusters:")
    for k, v in sorted(total_clusters.items(), key=lambda x: -x[1]):
        print(f"  {k}: {v}")
    
    print(f"\nReport saved to: {report_path}")
    
    # Generate WorkOrders for dominant clusters
    workorders_dir = output_dir / "workorders"
    workorders_dir.mkdir(exist_ok=True)
    
    print("\n--- Auto-Generated WorkOrders ---")
    workorders_created = generate_workorders(total_clusters, metrics["worst_3_cases"], workorders_dir)
    for wo in workorders_created:
        print(f"  Created: {wo}")
    
    # Generate failure packets for top 3 worst cases
    packets_dir = output_dir / "failure_packets"
    packets_dir.mkdir(exist_ok=True)
    
    print("\n--- Generating Failure Packets for Top 3 Cases ---")
    all_results = {r["case_id"]: r for r in builtin_results + websuite_results}
    for worst in metrics["worst_3_cases"]:
        case_id = worst["case_id"]
        result = all_results.get(case_id)
        if result and result.get("success"):
            packet_path = generate_failure_packet(
                case_id,
                result,
                packets_dir,
            )
            if packet_path:
                print(f"  Generated: {packet_path}")
    
    # Determine overall parity estimate
    parity_estimate = 100 - metrics["tier_b_weighted_mean"]
    print(f"\n>>> ESTIMATED PARITY: {parity_estimate:.1f}% <<<")
    
    # Auto-archive the run
    if auto_archive:
        print("\n--- Auto-Archiving Run ---")
        run_dir = archive_run(output_dir, history_dir, tag)
        if run_dir:
            print(f"  Archived to: {run_dir}")
            
            # Compare to previous run
            prev_run_id = get_previous_run(history_dir, run_dir.name)
            if prev_run_id:
                print("\n--- Comparison to Previous Run ---")
                prev_summary_path = history_dir / prev_run_id / "summary.json"
                curr_summary_path = run_dir / "summary.json"
                
                if prev_summary_path.exists() and curr_summary_path.exists():
                    with open(prev_summary_path) as f:
                        prev_summary = json.load(f)
                    with open(curr_summary_path) as f:
                        curr_summary = json.load(f)
                    
                    prev_parity = prev_summary["estimated_parity"]
                    curr_parity = curr_summary["estimated_parity"]
                    delta = curr_parity - prev_parity
                    
                    indicator = "▲" if delta > 0 else "▼" if delta < 0 else "="
                    status = "IMPROVED" if delta > 0 else "REGRESSED" if delta < 0 else "UNCHANGED"
                    
                    print(f"  Previous: {prev_parity:.1f}%")
                    print(f"  Current:  {curr_parity:.1f}%")
                    print(f"  Change:   {indicator} {delta:+.1f}% {status}")
                    
                    # Show significant case changes
                    prev_cases = prev_summary.get("case_diffs", {})
                    curr_cases = curr_summary.get("case_diffs", {})
                    
                    significant_changes = []
                    for case_id in set(prev_cases.keys()) | set(curr_cases.keys()):
                        prev_diff = prev_cases.get(case_id, {}).get("diff_pct", 100)
                        curr_diff = curr_cases.get(case_id, {}).get("diff_pct", 100)
                        case_delta = curr_diff - prev_diff
                        if abs(case_delta) >= 5:
                            significant_changes.append((case_id, prev_diff, curr_diff, case_delta))
                    
                    if significant_changes:
                        print("\n  Significant Case Changes (>5%):")
                        for case_id, prev_diff, curr_diff, case_delta in sorted(significant_changes, key=lambda x: x[3]):
                            ind = "✓" if case_delta < 0 else "✗"
                            print(f"    {case_id}: {prev_diff:.1f}% -> {curr_diff:.1f}% ({case_delta:+.1f}%) {ind}")
        else:
            print("  Failed to archive run")
    
    print("\n" + "=" * 60)
    print("Run complete!")
    if auto_archive:
        print("Use 'python3 scripts/parity_compare.py' to see full comparison")
        print("Use 'python3 scripts/parity_summary.py' to see trend report")
    print("=" * 60)


def generate_failure_packet(case_id: str, result: Dict[str, Any], output_dir: Path) -> Optional[str]:
    """Generate a failure packet for a specific case."""
    packet_dir = output_dir / case_id
    packet_dir.mkdir(exist_ok=True)
    
    packet = {
        "case_id": case_id,
        "generated_at": datetime.now().isoformat(),
        "estimated_diff_pct": result.get("estimated_diff_pct", 100),
        "html_path": result.get("html_path"),
        "dimensions": {
            "width": result.get("width"),
            "height": result.get("height"),
        },
    }
    
    # Copy frame if available
    frame_path = result.get("frame_path")
    if frame_path and Path(frame_path).exists():
        dest_frame = packet_dir / "rustkit_frame.ppm"
        import shutil
        shutil.copy(frame_path, dest_frame)
        packet["rustkit_frame"] = str(dest_frame)
    
    # Include layout stats
    layout_stats = result.get("layout_stats", {})
    if layout_stats:
        packet["layout_stats"] = layout_stats
    
    # Include issue clusters
    issue_clusters = result.get("issue_clusters", {})
    if issue_clusters:
        packet["issue_clusters"] = issue_clusters
    
    # Identify dominant issue
    if issue_clusters:
        dominant = max(issue_clusters.items(), key=lambda x: x[1])
        packet["dominant_issue"] = dominant[0]
        packet["dominant_count"] = dominant[1]
    
    # Include perf data if available
    perf = result.get("perf", {})
    if perf:
        packet["perf"] = perf
    
    # Save packet manifest
    manifest_path = packet_dir / "manifest.json"
    with open(manifest_path, "w") as f:
        json.dump(packet, f, indent=2)
    
    return str(packet_dir)


def generate_workorders(clusters: Dict[str, int], worst_cases: List[Dict], output_dir: Path) -> List[str]:
    """Generate WorkOrders based on failure clusters."""
    created = []
    
    # Find the dominant cluster
    sorted_clusters = sorted(clusters.items(), key=lambda x: -x[1])
    
    for cluster_name, count in sorted_clusters:
        if count == 0:
            continue
        
        # Create WorkOrder for this cluster
        workorder = {
            "id": f"parity-{cluster_name}-{datetime.now().strftime('%Y%m%d')}",
            "title": f"Fix {cluster_name.replace('_', ' ').title()} Issues",
            "description": f"Address {count} {cluster_name} issues identified in parity baseline.",
            "priority": "high" if count > 10 else "medium",
            "cluster": cluster_name,
            "issue_count": count,
            "affected_cases": [c["case_id"] for c in worst_cases if cluster_name in str(c)],
            "acceptance_criteria": [
                f"Reduce {cluster_name} issue count by at least 50%",
                "No regression in other clusters",
                "Tier A pass rate improves",
            ],
            "created": datetime.now().isoformat(),
        }
        
        wo_path = output_dir / f"{cluster_name}.json"
        with open(wo_path, "w") as f:
            json.dump(workorder, f, indent=2)
        
        created.append(str(wo_path))
    
    # Create a summary WorkOrder for the top 3 worst cases
    if worst_cases:
        summary_wo = {
            "id": f"parity-top-failures-{datetime.now().strftime('%Y%m%d')}",
            "title": "Fix Top 3 Worst Parity Cases",
            "description": "Focus on the three cases with highest pixel diff.",
            "priority": "critical",
            "cases": worst_cases,
            "acceptance_criteria": [
                f"Reduce diff% for {worst_cases[0]['case_id']} below 25%",
                "All three cases show measurable improvement",
            ],
            "created": datetime.now().isoformat(),
        }
        
        wo_path = output_dir / "top_failures.json"
        with open(wo_path, "w") as f:
            json.dump(summary_wo, f, indent=2)
        
        created.append(str(wo_path))
    
    return created


if __name__ == "__main__":
    main()

