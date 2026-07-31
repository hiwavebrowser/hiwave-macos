#!/usr/bin/env python3
"""wpt_tier1.py — run the Tier-1 WPT reftest seed through the real engine.

W0b of WPT Phase 0.5. The contract (docs/WPT_W0B_IMPLEMENT_PIN_2026-07-29.md):

- The manifest is the source of truth; third_party/wpt/ is a projection of it
  (scripts/wpt_sync.sh). This runner never fetches anything.
- Test and reference BOTH render through the same parity-capture binary the
  parity campaign uses, at the manifest viewport (800x600), passed explicitly
  on every call — never the binary's own 1280x800 default.
- A reftest passes when the test frame and the reference frame match. This is
  HiWave-vs-HiWave, not HiWave-vs-Chrome: WPT asks "does the engine agree with
  itself where the spec says two documents must render identically."
- <link rel="match"> inside the test file is the authority on which reference
  to use. If it disagrees with the manifest, that is an INSTRUMENT error and
  no pixels are compared — a diff against the wrong reference is not data.
- A blank frame is never a match. Two blank frames agree with each other for
  the worst possible reason (the engine rendered neither document), which is
  the empty-capture-scores-100 lie wearing a reftest costume. Blank -> ERROR.
- The oracle must be able to go red. Every run starts by rendering a
  deliberately mismatched fixture pair (trench/wpt/negative-control/); if the
  harness cannot fail THAT, the run aborts and publishes nothing.

Exit codes: 0 = ran and wrote last-run.json (individual cases may FAIL — that
is a result, not a runner error); 1 = harness-level failure (missing binary,
sync not run, negative control did not go red).
"""

import argparse
import json
import re
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from parity_lib import analyze_frame_blankness  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parent.parent
MANIFEST_PATH = REPO_ROOT / "trench" / "wpt" / "MANIFEST.json"
WPT_ROOT = REPO_ROOT / "third_party" / "wpt"
CAPTURE_BIN = REPO_ROOT / "target" / "release" / "parity-capture"
LAST_RUN_PATH = REPO_ROOT / "trench" / "wpt" / "last-run.json"
NEGATIVE_CONTROL_DIR = REPO_ROOT / "trench" / "wpt" / "negative-control"

# The single threshold knob. WPT reftest semantics are "renders identically",
# so W0b starts at exact RGB match. If antialiasing noise ever forces a
# tolerance, it gets raised HERE, documented in last-run.json, with Pete's
# sign-off — never borrowed from the campaign's t15.
WPT_MAX_DIFF_PCT = 0.0

REL_MATCH_RE = re.compile(
    r"<link\b[^>]*\brel\s*=\s*[\"']?match[\"']?[^>]*>", re.IGNORECASE
)
HREF_RE = re.compile(r"\bhref\s*=\s*[\"']([^\"']+)[\"']", re.IGNORECASE)


def read_ppm(path: Path):
    """(width, height, pixel bytes) from a binary P6 PPM."""
    data = path.read_bytes()
    if not data.startswith(b"P6"):
        raise ValueError(f"{path}: not a P6 PPM")
    # Header: magic, whitespace/comments, width, height, maxval, single ws.
    pos, fields = 2, []
    while len(fields) < 3:
        while pos < len(data) and data[pos : pos + 1].isspace():
            pos += 1
        if data[pos : pos + 1] == b"#":
            while pos < len(data) and data[pos : pos + 1] != b"\n":
                pos += 1
            continue
        start = pos
        while pos < len(data) and not data[pos : pos + 1].isspace():
            pos += 1
        fields.append(int(data[start:pos]))
    pos += 1  # single whitespace after maxval
    width, height, _maxval = fields
    pixels = data[pos : pos + width * height * 3]
    if len(pixels) != width * height * 3:
        raise ValueError(f"{path}: truncated pixel data")
    return width, height, pixels


def compare_frames(test_ppm: Path, ref_ppm: Path):
    """(diff_pixels, diff_pct). Dimension mismatch is a full-frame diff."""
    tw, th, tp = read_ppm(test_ppm)
    rw, rh, rp = read_ppm(ref_ppm)
    if (tw, th) != (rw, rh):
        total = max(tw * th, rw * rh)
        return total, 100.0
    total = tw * th
    diff = sum(
        1
        for i in range(0, len(tp), 3)
        if tp[i : i + 3] != rp[i : i + 3]
    )
    return diff, (diff / total * 100.0) if total else 100.0


def rel_match_hrefs(test_file: Path):
    """All <link rel=match> hrefs in a test file, in document order."""
    html = test_file.read_text(errors="replace")
    return [
        m.group(1)
        for link in REL_MATCH_RE.finditer(html)
        if (m := HREF_RE.search(link.group(0)))
    ]


def capture(html_file: Path, out_ppm: Path, viewport):
    """Render through the engine. Returns None on success, reason on failure."""
    cmd = [
        str(CAPTURE_BIN),
        "--html-file", str(html_file),
        "--width", str(viewport["width"]),
        "--height", str(viewport["height"]),
        "--dump-frame", str(out_ppm),
    ]
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True, timeout=60)
    except subprocess.TimeoutExpired:
        return "capture timeout (60s)"
    if proc.returncode != 0:
        tail = (proc.stderr or proc.stdout or "").strip().splitlines()
        return f"capture exit {proc.returncode}: {tail[-1] if tail else 'no output'}"
    if not out_ppm.exists() or out_ppm.stat().st_size == 0:
        return "capture produced no frame"
    return None


def run_pair(test_file: Path, ref_file: Path, viewport, workdir: Path):
    """Render both sides and compare. Returns a partial case dict."""
    test_ppm = workdir / "test.ppm"
    ref_ppm = workdir / "ref.ppm"

    for label, src, dst in (("test", test_file, test_ppm),
                            ("ref", ref_file, ref_ppm)):
        reason = capture(src, dst, viewport)
        if reason:
            return {"status": "ERROR", "reason": f"{label}: {reason}"}

    # Blank gate. Two blank frames "match" because the engine rendered
    # neither document — that is a refusal, not a pass.
    blanks = []
    for label, ppm in (("test", test_ppm), ("ref", ref_ppm)):
        b = analyze_frame_blankness(ppm, (255, 255, 255))
        if b.get("is_blank"):
            blanks.append(label)
    if blanks:
        return {
            "status": "ERROR",
            "reason": f"blank frame ({'+'.join(blanks)}) — render refusal, not a match",
        }

    diff_pixels, diff_pct = compare_frames(test_ppm, ref_ppm)
    status = "PASS" if diff_pct <= WPT_MAX_DIFF_PCT else "FAIL"
    return {
        "status": status,
        "reason": None,
        "diff_pct": round(diff_pct, 4),
        "diff_pixels": diff_pixels,
    }


def negative_control(viewport) -> bool:
    """Prove the oracle can go red. True = it failed the mismatched pair."""
    test = NEGATIVE_CONTROL_DIR / "control.html"
    notref = NEGATIVE_CONTROL_DIR / "control-notref.html"
    if not test.exists() or not notref.exists():
        print("HARNESS ERROR: negative-control fixtures missing", file=sys.stderr)
        return False
    with tempfile.TemporaryDirectory(prefix="wpt-negctl-") as td:
        result = run_pair(test, notref, viewport, Path(td))
    if result["status"] != "FAIL":
        print(
            f"HARNESS ERROR: negative control returned {result['status']} "
            f"({result.get('reason')}) — a harness that cannot fail a deliberate "
            f"mismatch publishes nothing",
            file=sys.stderr,
        )
        return False
    print(f"negative control: FAIL as required "
          f"(diff {result['diff_pct']}%) — oracle can go red")
    return True


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--case", help="run a single manifest id")
    parser.add_argument("--verbose", "-v", action="store_true")
    args = parser.parse_args()

    manifest = json.loads(MANIFEST_PATH.read_text())
    viewport = manifest["default_viewport"]
    entries = manifest["entries"]
    if args.case:
        entries = [e for e in entries if e["id"] == args.case]
        if not entries:
            print(f"no manifest entry with id {args.case}", file=sys.stderr)
            return 1

    if not CAPTURE_BIN.exists():
        print(f"parity-capture not built: {CAPTURE_BIN}\n"
              f"run: cargo build --release -p parity-capture", file=sys.stderr)
        return 1

    check = subprocess.run(
        [str(REPO_ROOT / "scripts" / "wpt_sync.sh"), "--check"],
        capture_output=True, text=True,
    )
    if check.returncode != 0:
        print("wpt_sync.sh --check failed — sync before running:\n"
              + check.stdout + check.stderr, file=sys.stderr)
        return 1

    if not negative_control(viewport):
        return 1

    hiwave_sha = subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=REPO_ROOT,
        capture_output=True, text=True,
    ).stdout.strip()

    cases = []
    for entry in entries:
        case = {
            "id": entry["id"],
            "status": None,
            "reason": None,
            "diff_pct": None,
            "diff_pixels": None,
            "rel_match": None,
            "maps_to": entry.get("maps_to"),
        }
        test_file = WPT_ROOT / entry["path"]
        ref_file = WPT_ROOT / entry["ref"] if entry.get("ref") else None

        if entry.get("kind") != "reftest":
            case.update(status="SKIP", reason=f"kind={entry.get('kind')} not run in W0b")
        elif not test_file.exists():
            case.update(status="ERROR", rel_match="missing",
                        reason="test file absent after --check passed (instrument)")
        elif ref_file is None or not ref_file.exists():
            case.update(status="SKIP", rel_match="missing", reason="missing ref")
        else:
            # rel=match is the authority; the manifest ref is a candidate.
            hrefs = rel_match_hrefs(test_file)
            if not hrefs:
                case.update(status="ERROR", rel_match="missing",
                            reason="test has no <link rel=match> — not a reftest at this pin")
            else:
                resolved = [
                    (test_file.parent / h).resolve().relative_to(WPT_ROOT.resolve())
                    for h in hrefs
                ]
                if ref_file.resolve().relative_to(WPT_ROOT.resolve()) not in resolved:
                    case.update(
                        status="ERROR", rel_match="mismatch",
                        reason=f"manifest ref {entry['ref']} not among rel=match "
                               f"targets {[str(r) for r in resolved]} — instrument error, "
                               f"pixels not compared",
                    )
                else:
                    case["rel_match"] = "ok"
                    with tempfile.TemporaryDirectory(prefix="wpt-") as td:
                        case.update(run_pair(test_file, ref_file, viewport, Path(td)))

        cases.append(case)
        if args.verbose or case["status"] != "PASS":
            detail = case["reason"] or f"diff {case['diff_pct']}%"
            print(f"  {case['id']:55s} {case['status']:5s} {detail}")
        else:
            print(f"  {case['id']:55s} PASS")

    counts = {s: sum(1 for c in cases if c["status"] == s)
              for s in ("PASS", "FAIL", "SKIP", "ERROR")}
    scored = counts["PASS"] + counts["FAIL"]
    rate = round(counts["PASS"] / scored, 4) if scored else None

    last_run = {
        "schema": 1,
        "wpt_pin": manifest["wpt_pin"],
        "hiwave_git_sha": hiwave_sha,
        "runner": "scripts/wpt_tier1.py",
        "viewport": viewport,
        "wpt_max_diff_pct": WPT_MAX_DIFF_PCT,
        "ts": datetime.now(timezone.utc).isoformat(),
        "n": len(cases),
        "pass": counts["PASS"],
        "fail": counts["FAIL"],
        "skip": counts["SKIP"],
        "error": counts["ERROR"],
        "rate": rate,
        "cases": cases,
        "honesty": {
            "all_green_suspect": True,
            "negative_control": "FAIL as required (checked every run; run aborts otherwise)",
            "note": "First run with pass==n and fail==0 and error==0 is presumptively "
                    "a lying harness unless N is tiny exploratory — W0b exit forbids "
                    "that shape for full seed.",
        },
    }

    if not args.case:
        LAST_RUN_PATH.write_text(json.dumps(last_run, indent=2) + "\n")
        print(f"\nwrote {LAST_RUN_PATH.relative_to(REPO_ROOT)}")

    print(f"\nWPT Tier-1: {counts['PASS']}/{scored} scored "
          f"(pass {counts['PASS']} / fail {counts['FAIL']} / "
          f"skip {counts['SKIP']} / error {counts['ERROR']}, n={len(cases)}) "
          f"@ pin {manifest['wpt_pin'][:12]}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
