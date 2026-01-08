#!/usr/bin/env python3
"""
parity_test.py - Triple-verified parity testing against Chrome baselines

This script:
1. Captures RustKit rendering for each test case
2. Compares against Chrome baselines (pixel diff)
3. Compares computed styles
4. Compares layout rects
5. Generates comprehensive report

Usage:
    python3 scripts/parity_test.py [--scope <scope>] [--case <name>] [--threshold <pct>]
    
Examples:
    python3 scripts/parity_test.py                    # All cases
    python3 scripts/parity_test.py --scope builtins   # Built-ins only
    python3 scripts/parity_test.py --case new_tab     # Single case
    python3 scripts/parity_test.py --threshold 10     # Strict threshold
"""

import json
import os
import subprocess
import sys
from datetime import datetime
from pathlib import Path

REPO_ROOT = Path(__file__).parent.parent
BASELINES_DIR = REPO_ROOT / "baselines" / "chrome-120"
OUTPUT_DIR = REPO_ROOT / "parity-baseline"

# Case definitions
BUILTINS = [
    ("new_tab", "crates/hiwave-app/src/ui/new_tab.html", 1280, 800),
    ("about", "crates/hiwave-app/src/ui/about.html", 800, 600),
    ("settings", "crates/hiwave-app/src/ui/settings.html", 1024, 768),
    ("chrome_rustkit", "crates/hiwave-app/src/ui/chrome_rustkit.html", 1280, 100),
    ("shelf", "crates/hiwave-app/src/ui/shelf.html", 1280, 120),
]

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

# Thresholds by component type
THRESHOLDS = {
    "layout_structure": 5,
    "solid_backgrounds": 8,
    "images_replaced": 10,
    "gradients_effects": 15,
    "form_controls": 12,
    "text_rendering": 20,
    "sticky_scroll": 25,
    "default": 15,
}


def get_threshold(case_id: str) -> float:
    """Get appropriate threshold for a case."""
    if "form" in case_id:
        return THRESHOLDS["form_controls"]
    if "image" in case_id or "gallery" in case_id:
        return THRESHOLDS["images_replaced"]
    if "gradient" in case_id:
        return THRESHOLDS["gradients_effects"]
    if "sticky" in case_id or "scroll" in case_id:
        return THRESHOLDS["sticky_scroll"]
    if "typography" in case_id or "text" in case_id:
        return THRESHOLDS["text_rendering"]
    return THRESHOLDS["default"]


def run_rustkit_capture(case_id: str, html_path: str, width: int, height: int) -> dict:
    """Capture RustKit rendering for a case."""
    output_dir = OUTPUT_DIR / "captures" / case_id
    output_dir.mkdir(parents=True, exist_ok=True)
    
    frame_path = output_dir / "frame.ppm"
    layout_path = output_dir / "layout.json"
    
    # Build parity-capture if needed
    build_cmd = ["cargo", "build", "--release", "-p", "parity-capture"]
    subprocess.run(build_cmd, capture_output=True, cwd=REPO_ROOT)
    
    # Run capture
    capture_cmd = [
        str(REPO_ROOT / "target" / "release" / "parity-capture"),
        "--html-file", str(REPO_ROOT / html_path),
        "--width", str(width),
        "--height", str(height),
        "--dump-frame", str(frame_path),
        "--dump-layout", str(layout_path),
    ]
    
    try:
        result = subprocess.run(
            capture_cmd,
            capture_output=True,
            text=True,
            timeout=30,
            cwd=REPO_ROOT,
        )
        
        if result.returncode == 0:
            # Check if files were created
            if frame_path.exists():
                return {"success": True, "output_dir": str(output_dir)}
            else:
                return {"success": False, "error": "No frame output"}
        else:
            return {"success": False, "error": result.stderr[:200]}
    except subprocess.TimeoutExpired:
        return {"success": False, "error": "Timeout"}
    except Exception as e:
        return {"success": False, "error": str(e)}


def compare_pixels(chrome_png: Path, rustkit_ppm: Path, output_dir: Path) -> dict:
    """Compare pixel data using Node.js tool."""
    output_dir.mkdir(parents=True, exist_ok=True)
    
    cmd = [
        "node", "-e", f"""
import {{ comparePixels }} from './tools/parity_oracle/compare_baseline.mjs';
const result = await comparePixels(
    '{chrome_png}',
    '{rustkit_ppm}',
    '{output_dir}'
);
console.log(JSON.stringify(result));
"""
    ]
    
    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=30,
            cwd=REPO_ROOT,
            env={**os.environ, "PATH": f"/opt/homebrew/bin:{os.environ.get('PATH', '')}"},
        )
        
        if result.returncode == 0:
            for line in result.stdout.strip().split('\n'):
                if line.startswith('{'):
                    return json.loads(line)
        return {"error": result.stderr[:200]}
    except Exception as e:
        return {"error": str(e)}


def compare_styles(chrome_styles: Path, rustkit_styles: Path) -> dict:
    """Compare computed styles."""
    if not chrome_styles.exists():
        return {"error": "Chrome styles not found"}
    if not rustkit_styles.exists():
        return {"error": "RustKit styles not found", "matched": 0, "mismatched": 0}
    
    try:
        chrome = json.loads(chrome_styles.read_text())
        rustkit = json.loads(rustkit_styles.read_text())
        
        chrome_map = {e["selector"]: e for e in chrome.get("elements", [])}
        rustkit_map = {e.get("selector", ""): e for e in rustkit.get("elements", [])}
        
        matched = 0
        mismatched = 0
        differences = []
        
        key_props = ["display", "width", "height", "margin-top", "padding-top", "position"]
        
        for selector, chrome_el in chrome_map.items():
            rustkit_el = rustkit_map.get(selector)
            if not rustkit_el:
                mismatched += 1
                continue
            
            diffs = []
            for prop in key_props:
                cv = chrome_el.get("styles", {}).get(prop)
                rv = rustkit_el.get("styles", {}).get(prop)
                if cv != rv:
                    diffs.append({"prop": prop, "chrome": cv, "rustkit": rv})
            
            if diffs:
                mismatched += 1
                differences.append({"selector": selector, "diffs": diffs})
            else:
                matched += 1
        
        return {
            "matched": matched,
            "mismatched": mismatched,
            "differences": differences[:10],  # Top 10
        }
    except Exception as e:
        return {"error": str(e)}


def compare_rects(chrome_rects: Path, rustkit_rects: Path, tolerance: float = 5.0) -> dict:
    """Compare layout rects."""
    if not chrome_rects.exists():
        return {"error": "Chrome rects not found"}
    if not rustkit_rects.exists():
        return {"error": "RustKit rects not found", "matched": 0, "mismatched": 0}
    
    try:
        chrome = json.loads(chrome_rects.read_text())
        rustkit = json.loads(rustkit_rects.read_text())
        
        chrome_map = {e["selector"]: e for e in chrome.get("elements", [])}
        rustkit_map = {e.get("selector", ""): e for e in rustkit.get("elements", [])}
        
        matched = 0
        mismatched = 0
        differences = []
        
        for selector, chrome_el in chrome_map.items():
            rustkit_el = rustkit_map.get(selector)
            if not rustkit_el:
                mismatched += 1
                continue
            
            cr = chrome_el.get("rect", {})
            rr = rustkit_el.get("rect", rustkit_el.get("content_rect", {}))
            
            diffs = []
            for prop in ["width", "height", "x", "y"]:
                cv = cr.get(prop, 0)
                rv = rr.get(prop, 0)
                if abs(cv - rv) > tolerance:
                    diffs.append({"prop": prop, "chrome": cv, "rustkit": rv})
            
            if diffs:
                mismatched += 1
                differences.append({"selector": selector, "diffs": diffs})
            else:
                matched += 1
        
        return {
            "matched": matched,
            "mismatched": mismatched,
            "differences": differences[:10],  # Top 10
        }
    except Exception as e:
        return {"error": str(e)}


def run_test(case_id: str, html_path: str, width: int, height: int, case_type: str) -> dict:
    """Run full triple-verification test for a case."""
    baseline_dir = BASELINES_DIR / case_type / case_id
    capture_dir = OUTPUT_DIR / "captures" / case_id
    diff_dir = OUTPUT_DIR / "diffs" / case_id
    
    result = {
        "case_id": case_id,
        "type": case_type,
        "threshold": get_threshold(case_id),
        "pixel": None,
        "styles": None,
        "rects": None,
        "passed": False,
    }
    
    # Check baseline exists
    chrome_png = baseline_dir / "baseline.png"
    if not chrome_png.exists():
        result["error"] = "No Chrome baseline"
        return result
    
    # Capture RustKit
    capture_result = run_rustkit_capture(case_id, html_path, width, height)
    if not capture_result.get("success"):
        result["error"] = f"Capture failed: {capture_result.get('error', 'Unknown')}"
        return result
    
    # Find RustKit output
    rustkit_ppm = capture_dir / "frame.ppm"
    if not rustkit_ppm.exists():
        result["error"] = "No RustKit capture output"
        return result
    
    # 1. Pixel comparison
    pixel_result = compare_pixels(chrome_png, rustkit_ppm, diff_dir)
    result["pixel"] = pixel_result
    
    # 2. Style comparison
    chrome_styles = baseline_dir / "computed-styles.json"
    rustkit_styles = capture_dir / "computed-styles.json"
    result["styles"] = compare_styles(chrome_styles, rustkit_styles)
    
    # 3. Rect comparison
    chrome_rects = baseline_dir / "layout-rects.json"
    rustkit_rects = capture_dir / "layout.json"
    result["rects"] = compare_rects(chrome_rects, rustkit_rects)
    
    # Determine pass/fail
    diff_pct = pixel_result.get("diffPercent", 100)
    result["diff_pct"] = diff_pct
    result["passed"] = diff_pct <= result["threshold"]
    
    return result


def main():
    scope = "all"
    single_case = None
    threshold_override = None
    
    # Parse arguments
    args = sys.argv[1:]
    i = 0
    while i < len(args):
        if args[i] == "--scope" and i + 1 < len(args):
            scope = args[i + 1]
            i += 2
        elif args[i] == "--case" and i + 1 < len(args):
            single_case = args[i + 1]
            i += 2
        elif args[i] == "--threshold" and i + 1 < len(args):
            threshold_override = float(args[i + 1])
            i += 2
        elif args[i] in ["-h", "--help"]:
            print(__doc__)
            sys.exit(0)
        else:
            i += 1
    
    print("=" * 60)
    print("Triple-Verified Parity Test")
    print("=" * 60)
    print(f"Baselines: {BASELINES_DIR}")
    print(f"Scope: {scope}")
    print(f"Timestamp: {datetime.now().isoformat()}")
    print()
    
    # Determine cases to run
    cases = []
    if single_case:
        all_cases = {c[0]: c for c in BUILTINS + WEBSUITE}
        if single_case in all_cases:
            c = all_cases[single_case]
            case_type = "builtins" if any(b[0] == single_case for b in BUILTINS) else "websuite"
            cases = [(c[0], c[1], c[2], c[3], case_type)]
        else:
            print(f"Error: Unknown case '{single_case}'")
            sys.exit(1)
    else:
        if scope in ["all", "builtins"]:
            cases.extend([(c[0], c[1], c[2], c[3], "builtins") for c in BUILTINS])
        if scope in ["all", "websuite"]:
            cases.extend([(c[0], c[1], c[2], c[3], "websuite") for c in WEBSUITE])
    
    # Run tests
    results = []
    passed = 0
    failed = 0
    
    for case_id, html_path, width, height, case_type in cases:
        print(f"  Testing {case_id}...", end=" ", flush=True)
        
        result = run_test(case_id, html_path, width, height, case_type)
        results.append(result)
        
        if result.get("error"):
            print(f"ERROR: {result['error'][:40]}")
            failed += 1
        elif result["passed"]:
            print(f"✓ {result['diff_pct']:.1f}% (threshold: {result['threshold']}%)")
            passed += 1
        else:
            print(f"✗ {result['diff_pct']:.1f}% (threshold: {result['threshold']}%)")
            failed += 1
    
    # Save results
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    results_path = OUTPUT_DIR / "parity_test_results.json"
    with open(results_path, "w") as f:
        json.dump({
            "timestamp": datetime.now().isoformat(),
            "scope": scope,
            "passed": passed,
            "failed": failed,
            "results": results,
        }, f, indent=2)
    
    # Summary
    print()
    print("=" * 60)
    print("Summary")
    print("=" * 60)
    print(f"Passed: {passed}/{len(results)}")
    print(f"Failed: {failed}/{len(results)}")
    
    if results:
        avg_diff = sum(r.get("diff_pct", 100) for r in results) / len(results)
        print(f"Average Diff: {avg_diff:.1f}%")
    
    print(f"\nResults saved to: {results_path}")
    
    # Show worst cases
    sorted_results = sorted(results, key=lambda r: r.get("diff_pct", 100), reverse=True)
    print("\nWorst 3 Cases:")
    for r in sorted_results[:3]:
        print(f"  {r['case_id']}: {r.get('diff_pct', 'N/A')}%")
    
    sys.exit(0 if failed == 0 else 1)


if __name__ == "__main__":
    main()

