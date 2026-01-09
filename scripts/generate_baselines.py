#!/usr/bin/env python3
"""
generate_baselines.py - Generate Chrome baselines for visual parity testing

This script captures Chrome baselines for all test cases:
- baseline.png: Screenshot
- computed-styles.json: CSS computed values
- layout-rects.json: DOMRect for all elements

Usage:
    python3 scripts/generate_baselines.py [--scope <scope>] [--case <name>]
    
Examples:
    python3 scripts/generate_baselines.py                    # All cases
    python3 scripts/generate_baselines.py --scope builtins   # Built-ins only
    python3 scripts/generate_baselines.py --case new_tab     # Single case
"""

import json
import os
import subprocess
import sys
from datetime import datetime
from pathlib import Path

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

REPO_ROOT = Path(__file__).parent.parent
BASELINES_DIR = REPO_ROOT / "baselines" / "chrome-120"
ORACLE_SCRIPT = REPO_ROOT / "tools" / "parity_oracle" / "capture_baseline.mjs"


def get_git_commit():
    """Get current git commit hash."""
    try:
        result = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            capture_output=True,
            text=True,
            timeout=5,
            cwd=REPO_ROOT,
        )
        if result.returncode == 0:
            return result.stdout.strip()[:12]
    except Exception:
        pass
    return "unknown"


def check_node_deps():
    """Check if Node.js dependencies are installed."""
    node_modules = REPO_ROOT / "tools" / "parity_oracle" / "node_modules"
    if not node_modules.exists():
        print("Error: Node.js dependencies not installed.")
        print("Run: cd tools/parity_oracle && npm install")
        return False
    return True


def capture_case(case_id: str, html_path: str, width: int, height: int, output_dir: Path) -> dict:
    """Capture baseline for a single case using Node.js oracle."""
    case_dir = output_dir / case_id
    case_dir.mkdir(parents=True, exist_ok=True)
    
    # Use Node.js capture_baseline.mjs
    cmd = [
        "node", "-e",
        f"""
import {{ captureBaseline }} from './capture_baseline.mjs';
const result = await captureBaseline(
    '{REPO_ROOT / html_path}',
    '{case_dir}',
    {width},
    {height}
);
console.log(JSON.stringify(result));
""",
    ]
    
    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=60,
            cwd=REPO_ROOT / "tools" / "parity_oracle",
        )
        
        if result.returncode == 0:
            # Parse JSON output
            for line in result.stdout.strip().split('\n'):
                if line.startswith('{'):
                    return json.loads(line)
            return {"success": True, "output_dir": str(case_dir)}
        else:
            return {"success": False, "error": result.stderr[:200]}
    except subprocess.TimeoutExpired:
        return {"success": False, "error": "Timeout after 60s"}
    except Exception as e:
        return {"success": False, "error": str(e)}


def main():
    scope = "all"
    single_case = None
    
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
        elif args[i] in ["-h", "--help"]:
            print(__doc__)
            sys.exit(0)
        else:
            i += 1
    
    # Check dependencies
    if not check_node_deps():
        sys.exit(1)
    
    print("=" * 60)
    print("Chrome Baseline Generator")
    print("=" * 60)
    print(f"Output: {BASELINES_DIR}")
    print(f"Scope: {scope}")
    if single_case:
        print(f"Single case: {single_case}")
    print(f"Timestamp: {datetime.now().isoformat()}")
    print()
    
    # Determine cases to run
    cases = []
    if single_case:
        # Find the case
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
    
    # Capture baselines
    results = {"builtins": {}, "websuite": {}}
    
    for case_id, html_path, width, height, case_type in cases:
        output_dir = BASELINES_DIR / case_type
        print(f"  Capturing {case_id}...", end=" ", flush=True)
        
        result = capture_case(case_id, html_path, width, height, output_dir)
        results[case_type][case_id] = result
        
        if result.get("success"):
            print(f"OK ({result.get('elementCount', '?')} elements)")
        else:
            print(f"FAIL: {result.get('error', 'Unknown')[:50]}")
    
    # Update metadata
    metadata_path = BASELINES_DIR.parent / "metadata.json"
    if metadata_path.exists():
        with open(metadata_path) as f:
            metadata = json.load(f)
    else:
        metadata = {}
    
    metadata["last_updated"] = datetime.now().isoformat()
    metadata["git_commit"] = get_git_commit()
    
    with open(metadata_path, "w") as f:
        json.dump(metadata, f, indent=2)
    
    # Summary
    print()
    print("=" * 60)
    print("Summary")
    print("=" * 60)
    
    total = 0
    passed = 0
    for case_type, case_results in results.items():
        for case_id, result in case_results.items():
            total += 1
            if result.get("success"):
                passed += 1
    
    print(f"Captured: {passed}/{total}")
    print(f"Metadata updated: {metadata_path}")
    print()
    
    if passed < total:
        print("Some captures failed. Check Node.js dependencies:")
        print("  cd tools/parity_oracle && npm install")
        sys.exit(1)


if __name__ == "__main__":
    main()



