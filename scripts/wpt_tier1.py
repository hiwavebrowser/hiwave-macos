#!/usr/bin/env python3
"""wpt_tier1.py — WPT Phase 0.5 W0b: the first honest Tier-1 K/N.

Renders each manifest reftest's test AND reference through the SAME headless
parity-capture path the campaign metric uses (design pin
trench/forensics/2026-07-15-wpt-phase05-GATE-OPEN.md, path P0 — no second
engine host), then compares the two frames EXACTLY. A WPT reftest passes only
if test and reference render pixel-identical; both sides come from RustKit, so
engine-deterministic AA cancels and exact match is the honest bar. No campaign
threshold, no pixelmatch fuzz.

Statuses:
  PASS       — frames pixel-identical
  FAIL       — frames differ (any nonzero pixel count; ratio reported)
  SKIP       — test not runnable by this harness, with reason (JS/reftest-wait
               dependence, fuzzy annotation, missing ref). A skip is not a fail.
  INSTRUMENT — the harness refused to measure (capture failed, dimension
               mismatch, tree/pin mismatch, rel=match disagrees with manifest).
               Never counted as a render result — the 2026-07-24 forensics
               (empty captures scored 100.0) is the lie class this guards.

rate = pass / (pass + fail). Skips and instrument refusals are excluded from
the denominator and reported alongside it; a rate quoted without its n is
nothing.

WPT serves '/' from the WPT root (wptserve); file:// has no such mapping, so
root-absolute references (e.g. /fonts/ahem.css) are rewritten in staged copies
that live NEXT TO the originals inside the gitignored tree — relative
references keep working, fixtures on disk stay verbatim. Referenced CSS gets
the same treatment for its url(...) values.

Writes trench/wpt/last-run.json. Exit 0 if every case measured (fails are a
meter reading, not an error); exit 2 if any INSTRUMENT refusal.

    python3 scripts/wpt_tier1.py [--filter SUBSTR] [--json]
"""

import argparse
import json
import re
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from parity_lib import (  # noqa: E402
    REPO_ROOT,
    analyze_frame_blankness,
    ensure_parity_capture_built,
    run_rustkit_capture,
)

MANIFEST_PATH = REPO_ROOT / "trench" / "wpt" / "MANIFEST.json"
LAST_RUN_PATH = REPO_ROOT / "trench" / "wpt" / "last-run.json"
WPT_ROOT = REPO_ROOT / "third_party" / "wpt"
RECEIPT_PATH = WPT_ROOT / ".sync-receipt.json"
RESULTS_ROOT = REPO_ROOT / "parity-results" / "wpt-tier1"

STAGED_SUFFIX = ".__staged.html"

REL_MATCH_RE = re.compile(
    r"<link[^>]*rel=[\"']?match[\"']?[^>]*href=[\"']?([^\"'> ]+)", re.IGNORECASE
)
# href="/x" / src="/x" — root-absolute only (skip //host and /Users-style paths we already wrote)
ROOT_ABS_ATTR_RE = re.compile(r"((?:href|src)=[\"'])(/(?!/)[^\"']*)([\"'])")
ROOT_ABS_URL_RE = re.compile(r"(url\([\"']?)(/(?!/)[^\"')]*)([\"']?\))")


def parse_ppm(path: Path):
    """Return (width, height, pixel_bytes) for a binary P6 PPM."""
    data = path.read_bytes()
    # Header: P6 <ws> width <ws> height <ws> maxval <single ws> raster
    m = re.match(rb"P6\s+(?:#[^\n]*\n\s*)*(\d+)\s+(\d+)\s+(\d+)\s", data)
    if not m:
        raise ValueError(f"not a binary P6 PPM: {path}")
    w, h = int(m.group(1)), int(m.group(2))
    raster = data[m.end() :]
    expected = w * h * 3
    if len(raster) < expected:
        raise ValueError(f"truncated PPM raster ({len(raster)} < {expected}): {path}")
    return w, h, raster[:expected]


def count_diff_pixels(a: bytes, b: bytes) -> int:
    n = 0
    for i in range(0, len(a), 3):
        if a[i : i + 3] != b[i : i + 3]:
            n += 1
    return n


def stage_css(src: Path) -> Path:
    """Copy a CSS file next to itself with root-absolute url(...) values made absolute."""
    staged = src.with_name(src.stem + ".__staged" + src.suffix)
    css = src.read_text(errors="replace")
    css = ROOT_ABS_URL_RE.sub(lambda m: m.group(1) + str(WPT_ROOT / m.group(2).lstrip("/")) + m.group(3), css)
    staged.write_text(css)
    return staged


def stage_html(src: Path) -> Path:
    """Copy an HTML file next to itself with root-absolute references resolved
    against the WPT root (what wptserve would have served). Referenced CSS is
    staged recursively so its url(...) values resolve too."""
    html = src.read_text(errors="replace")

    def rewrite(m):
        target = WPT_ROOT / m.group(2).lstrip("/")
        if target.suffix == ".css" and target.is_file():
            target = stage_css(target)
        return m.group(1) + str(target) + m.group(3)

    html = ROOT_ABS_ATTR_RE.sub(rewrite, html)
    html = ROOT_ABS_URL_RE.sub(lambda m: m.group(1) + str(WPT_ROOT / m.group(2).lstrip("/")) + m.group(3), html)
    staged = src.with_name(src.name[: -len(".html")] + STAGED_SUFFIX)
    staged.write_text(html)
    return staged


def skip_reason(test_src: str, ref_src: str):
    if "reftest-wait" in test_src or "reftest-wait" in ref_src:
        return "reftest-wait (needs scripted completion signal)"
    if "<script" in test_src or "<script" in ref_src:
        return "needs JS (no script host in parity-capture path)"
    if re.search(r"name=[\"']fuzzy[\"']", test_src):
        return "fuzzy annotation (tolerance matching not implemented)"
    return None


def git_sha() -> str:
    try:
        out = subprocess.run(
            ["git", "rev-parse", "HEAD"], capture_output=True, text=True, cwd=REPO_ROOT, timeout=10
        )
        return out.stdout.strip()[:12] or "unknown"
    except Exception:
        return "unknown"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--filter", default=None, help="only run entries whose id contains this substring")
    ap.add_argument("--json", action="store_true", help="print last-run.json to stdout at the end")
    args = ap.parse_args()

    manifest = json.loads(MANIFEST_PATH.read_text())
    pin = manifest["wpt_pin"]
    vp = manifest.get("default_viewport", {"width": 800, "height": 600})
    width, height = vp["width"], vp["height"]

    # Instrument preconditions: right tree at the right pin, engine built.
    if not RECEIPT_PATH.exists():
        print(f"INSTRUMENT: no sync receipt at {RECEIPT_PATH} — run scripts/wpt_sync.py first.", file=sys.stderr)
        return 2
    receipt_pin = json.loads(RECEIPT_PATH.read_text()).get("pin")
    if receipt_pin != pin:
        print(f"INSTRUMENT: tree pin {receipt_pin} != manifest pin {pin} — re-sync.", file=sys.stderr)
        return 2
    print("Building parity-capture (release)...")
    if not ensure_parity_capture_built():
        print("INSTRUMENT: parity-capture build failed.", file=sys.stderr)
        return 2

    run_dir = RESULTS_ROOT / datetime.now(timezone.utc).strftime("%Y%m%d_%H%M%S")
    run_dir.mkdir(parents=True, exist_ok=True)

    entries = manifest["entries"]
    if args.filter:
        entries = [e for e in entries if args.filter in e["id"]]

    cases = []
    for entry in entries:
        cid = entry["id"]
        started = time.time()
        rec = {"id": cid, "tier": entry["tier"], "maps_to": entry.get("maps_to"), "status": None, "reason": None}
        cases.append(rec)

        test_path = WPT_ROOT / entry["path"]
        ref_rel = entry.get("ref")

        if entry.get("kind") != "reftest":
            rec["status"], rec["reason"] = "SKIP", f"kind={entry.get('kind')} not runnable yet"
            continue
        if not test_path.is_file():
            rec["status"], rec["reason"] = "INSTRUMENT", f"test missing from tree: {entry['path']}"
            continue
        if not ref_rel or not (WPT_ROOT / ref_rel).is_file():
            rec["status"], rec["reason"] = "SKIP", f"missing ref: {ref_rel}"
            continue
        ref_path = WPT_ROOT / ref_rel

        test_src = test_path.read_text(errors="replace")
        ref_src = ref_path.read_text(errors="replace")

        # The test file's own <link rel=match> is WPT's authority for the
        # binding; the manifest's ref field is a listing-derived candidate.
        # Disagreement is an instrument error, not a render diff (W0a note).
        m = REL_MATCH_RE.search(test_src)
        if not m:
            rec["status"], rec["reason"] = "INSTRUMENT", "no <link rel=match> in test"
            continue
        href = m.group(1)
        resolved = (WPT_ROOT / href.lstrip("/")) if href.startswith("/") else (test_path.parent / href)
        if resolved.resolve() != ref_path.resolve():
            rec["status"], rec["reason"] = (
                "INSTRUMENT",
                f"rel=match {href!r} resolves to {resolved.relative_to(WPT_ROOT)} but manifest ref is {ref_rel}",
            )
            continue

        reason = skip_reason(test_src, ref_src)
        if reason:
            rec["status"], rec["reason"] = "SKIP", reason
            continue

        safe = cid.replace("/", "__")
        frames = {}
        for side, page in (("test", test_path), ("ref", ref_path)):
            staged = stage_html(page)
            frame = run_dir / f"{safe}.{side}.ppm"
            layout = run_dir / f"{safe}.{side}.layout.json"
            cap = run_rustkit_capture(str(staged), width, height, frame, layout)
            if not cap.get("success"):
                rec["status"], rec["reason"] = "INSTRUMENT", f"{side} capture failed: {cap.get('error')}"
                break
            frames[side] = frame
        if rec["status"]:
            continue

        try:
            tw, th, tdata = parse_ppm(frames["test"])
            rw, rh, rdata = parse_ppm(frames["ref"])
        except ValueError as e:
            rec["status"], rec["reason"] = "INSTRUMENT", str(e)
            continue
        if (tw, th) != (rw, rh):
            rec["status"], rec["reason"] = "INSTRUMENT", f"dimension mismatch test {tw}x{th} vs ref {rw}x{rh}"
            continue

        diff = count_diff_pixels(tdata, rdata)
        rec["diff_pixels"] = diff
        rec["diff_ratio_pct"] = round(diff / (tw * th) * 100, 4)
        # Blankness is recorded, not judged: an equal-blank pair can be a lying
        # pass on a content test — but also a CORRECT pass on the empty-span
        # cases, whose point is that nothing paints. The outside-eye reads this
        # field; the harness does not guess.
        rec["test_blank_ratio"] = round(analyze_frame_blankness(frames["test"]).get("background_ratio", 1.0), 4)
        rec["ref_blank_ratio"] = round(analyze_frame_blankness(frames["ref"]).get("background_ratio", 1.0), 4)
        rec["status"] = "PASS" if diff == 0 else "FAIL"
        rec["ms"] = int((time.time() - started) * 1000)

    n_pass = sum(1 for c in cases if c["status"] == "PASS")
    n_fail = sum(1 for c in cases if c["status"] == "FAIL")
    n_skip = sum(1 for c in cases if c["status"] == "SKIP")
    n_instr = sum(1 for c in cases if c["status"] == "INSTRUMENT")
    n = n_pass + n_fail
    rate = round(n_pass / n * 100, 1) if n else None

    result = {
        "pin": pin,
        "git_sha": git_sha(),
        "ts": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "viewport": {"width": width, "height": height},
        "n": n,
        "pass": n_pass,
        "fail": n_fail,
        "skip": n_skip,
        "instrument": n_instr,
        "rate_pct": rate,
        "all_green_suspect": bool(n and n_fail == 0),
        "cases": cases,
    }

    if not args.filter:
        LAST_RUN_PATH.write_text(json.dumps(result, indent=2) + "\n")
    else:
        print("(--filter run: last-run.json NOT written — partial runs are not the record)")

    print()
    for c in cases:
        detail = c["reason"] or (f"diff {c.get('diff_pixels')} px ({c.get('diff_ratio_pct')}%)" if c["status"] == "FAIL" else "")
        print(f"  {c['status']:<10} {c['id']}  {detail}")
    print()
    print(f"WPT Tier-1: {n_pass}/{n} ({rate}%) @ pin {pin[:7]} | skip {n_skip} | instrument {n_instr}")
    if result["all_green_suspect"]:
        print("WARNING: zero FAILs — an all-green first run means the harness is lying (lie #6 class). Verify before publishing.")
    if args.json:
        print(json.dumps(result, indent=2))
    return 2 if n_instr else 0


if __name__ == "__main__":
    sys.exit(main())
