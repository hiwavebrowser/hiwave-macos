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
- A reftest that cannot reach the state it asserts about is not scored.
  The capture path does not watch for reftest-wait's completion signal, so its
  frame may precede the state under test — and upstream WPT reports TIMEOUT
  there, not FAIL. Those are SKIP, with the reason recorded. (Added 2026-08-06
  night 20: three of the seed's six fails were this.) A page with plain inline
  scripts is scored, with script on (--run-inline-scripts, #799).
- A page whose local stylesheet, image or font is not on disk is an ERROR, no
  pixels compared. The sparse sync takes only listed paths, and both sides
  rendering without the same sheet can agree for the wrong reason (T14:
  css-flexbox/support/flexbox.css).
- `status` is the exact rule below. The campaign's rule (pixelmatch 0.1,
  15%) is reported beside it in `campaign_diff_pct`, and never decides a
  status: at 15% a page with a wrong line break still passes. Where it could
  not be measured, `campaign_not_measured` says why.
- A PASS that still matched with its named property removed (T14's probe) is
  listed under `unproven_passes`: it counts, but it is not yet proof of support.
- Root-absolute references (/fonts/ahem.css) are staged to resolve against the
  WPT root, because wptserve serves / from there and file:// does not. A
  reftest whose declared font silently fails to load is not measuring its own
  assertion. The fonts themselves are MANIFEST.support_paths.
- A fail whose test declares a web font is still a FAIL — @font-face is
  unimplemented, and that is an engine gap, not a harness one — but it is
  tagged `blocked_by` so a digest can name one capability instead of counting
  N unrelated text bugs.
- The oracle must be able to go red. Every run starts by rendering a
  deliberately mismatched fixture pair (trench/wpt/negative-control/); if the
  harness cannot fail THAT, the run aborts and publishes nothing.

Exit codes: 0 = ran and wrote last-run.json (individual cases may FAIL — that
is a result, not a runner error); 1 = harness-level failure (missing binary,
sync not run, negative control did not go red).
"""

import argparse
import json
import math
import os
import re
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from parity_lib import THRESHOLDS, analyze_frame_blankness  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parent.parent
MANIFEST_PATH = REPO_ROOT / "trench" / "wpt" / "MANIFEST.json"
WPT_ROOT = REPO_ROOT / "third_party" / "wpt"


def capture_bin(env=os.environ) -> Path:
    """Where the parity build lands: cargo's target dir, relative paths from the repo."""
    return REPO_ROOT / env.get("CARGO_TARGET_DIR", "target") / "parity" / "parity-capture"


def capture_label(path: Path) -> str:
    """The binary as last-run.json names it; a shared target dir is outside the repo."""
    return path.relative_to(REPO_ROOT).as_posix() if path.is_relative_to(REPO_ROOT) else str(path)


CAPTURE_BIN = capture_bin()
LAST_RUN_PATH = REPO_ROOT / "trench" / "wpt" / "last-run.json"
NEGATIVE_CONTROL_DIR = REPO_ROOT / "trench" / "wpt" / "negative-control"
ORACLE_DIR = REPO_ROOT / "tools" / "parity_oracle"

# The single threshold knob. WPT reftest semantics are "renders identically",
# so W0b starts at exact RGB match. If antialiasing noise ever forces a
# tolerance, it gets raised HERE, documented in last-run.json, with Pete's
# sign-off — never borrowed from the campaign's t15.
WPT_MAX_DIFF_PCT = 0.0
CAMPAIGN_PIXELMATCH_THRESHOLD = 0.1  # compare_baseline.mjs, wpt_pair_diff.mjs

REL_MATCH_RE = re.compile(
    r"<link\b[^>]*\brel\s*=\s*[\"']?match[\"']?[^>]*>", re.IGNORECASE
)


def _attr_re(name: str):
    """An attribute in any of HTML's three quotings, and not `data-<name>`."""
    return re.compile(
        rf"(?<![\w-]){name}\s*=\s*(?:\"([^\"]+)\"|'([^']+)'|([^\s\"'>]+))", re.IGNORECASE
    )


def _attr_value(m):
    return m.group(1) or m.group(2) or m.group(3)


HREF_RE = _attr_re("href")

# wptserve maps / to the WPT root; file:// has no such mapping, so root-absolute
# references (e.g. /fonts/ahem.css) resolve to the filesystem root and silently
# load nothing. Staged copies live NEXT TO the originals so relative references
# keep working and the fixtures on disk stay verbatim.
STAGED_SUFFIX = ".__staged"
LINK_RE = re.compile(r"<link\b[^>]*>", re.IGNORECASE)
REL_STYLESHEET_RE = re.compile(r"\brel\s*=\s*[\"']?[^\"'>]*\bstylesheet\b", re.IGNORECASE)
SRC_RE = _attr_re("src")
CSS_REF_RE = re.compile(
    r"@import\s+[\"']([^\"']+)[\"']|url\(\s*[\"']?([^\"')]+?)[\"']?\s*\)", re.IGNORECASE
)
ROOT_ABS_ATTR_RE = re.compile(
    r"((?<![\w-])(?:href|src)\s*=\s*)(?:([\"'])(/(?!/)[^\"']*)\2|(/(?!/)[^\s\"'>]+))", re.IGNORECASE
)
ROOT_ABS_URL_RE = re.compile(r"(url\(\s*[\"']?)(/(?!/)[^\"')]*)([\"']?\s*\))", re.IGNORECASE)
# Not resources: what comments, <meta content> and script text happen to say.
HTML_COMMENT_RE = re.compile(r"<!--.*?-->", re.DOTALL)
CSS_COMMENT_RE = re.compile(r"/\*.*?\*/", re.DOTALL)
META_RE = re.compile(r"<meta\b[^>]*>", re.IGNORECASE)
SCRIPT_BODY_RE = re.compile(r"(<script\b[^>]*>).*?(</script\s*>)", re.IGNORECASE | re.DOTALL)
TAG_RE = re.compile(r"<[a-z][^>]*>", re.IGNORECASE)

# A test that declares a web font is tagged so a digest can group its fails.
# Until n33 (2026-08-26) the tag read "@font-face unimplemented": FontLoader
# never fetched or registered anything, so every declared face fell back.
# Since n33 a TTF/OTF face loads and is what the shaper measures with (Ahem
# included). The tag is ATTRIBUTION ONLY and is deliberately kept as the same
# field so the trendline stays comparable: a fail here is a real engine
# failure measured IN the declared font, and a PASS is real if the face was
# accepted — the tag no longer implies either "blocked" or "suspect".
WEBFONT_GAP = "declares a web font (loads since n33: TTF/OTF; WOFF/WOFF2 not yet)"


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
        _attr_value(m)
        for link in REL_MATCH_RE.finditer(html)
        if (m := HREF_RE.search(link.group(0)))
    ]


def stage_css(src: Path) -> Path:
    """Copy a CSS file beside itself with root-absolute url(...) values resolved."""
    staged = src.with_name(src.stem + STAGED_SUFFIX + src.suffix)
    css = ROOT_ABS_URL_RE.sub(
        lambda m: m.group(1) + str(WPT_ROOT / m.group(2).lstrip("/")) + m.group(3),
        src.read_text(errors="replace"),
    )
    staged.write_text(css)
    return staged


def stage_html(src: Path) -> Path:
    """Copy an HTML file beside itself with root-absolute references resolved
    against the WPT root — what wptserve would have served. Referenced CSS is
    staged recursively so its own url(...) values resolve too."""
    def rewrite(m):
        target = WPT_ROOT / (m.group(3) or m.group(4)).lstrip("/")
        if target.suffix.lower() == ".css" and target.is_file():
            target = stage_css(target)
        quote = m.group(2) or '"'
        return m.group(1) + quote + str(target) + quote

    html = ROOT_ABS_ATTR_RE.sub(rewrite, src.read_text(errors="replace"))
    html = ROOT_ABS_URL_RE.sub(
        lambda m: m.group(1) + str(WPT_ROOT / m.group(2).lstrip("/")) + m.group(3), html
    )
    staged = src.with_name(src.stem + STAGED_SUFFIX + src.suffix)
    staged.write_text(html)
    return staged


def _is_local(url: str) -> bool:
    return not (re.match(r"[a-z][a-z0-9+.-]*:", url, re.IGNORECASE) or url.startswith(("//", "#")))


def _local_target(url: str, base_dir: Path, wpt_root: Path) -> Path:
    path = url.split("#")[0].split("?")[0]
    return wpt_root / path.lstrip("/") if path.startswith("/") else base_dir / path


def missing_resources(html_file: Path, wpt_root: Path):
    """Local subresources of a page that are not on disk, in the order found.

    Only what the page loads: <link rel=stylesheet>, src=, @import and url(),
    following linked sheets. <link rel=help|author|match> are not resources.
    """
    missing = []

    def note(url, base_dir):
        if not _is_local(url):
            return None
        target = _local_target(url, base_dir, wpt_root)
        if target.is_file():
            return target
        if url not in missing:
            missing.append(url)
        return None

    def scan_css(css, base_dir, seen):
        for m in CSS_REF_RE.finditer(CSS_COMMENT_RE.sub("", css)):
            url = m.group(1) or m.group(2)
            target = note(url, base_dir)
            if target and target.suffix.lower() == ".css" and target not in seen:
                seen.add(target)
                scan_css(target.read_text(errors="replace"), target.parent, seen)

    html = HTML_COMMENT_RE.sub("", html_file.read_text(errors="replace"))
    html = SCRIPT_BODY_RE.sub(r"\1\2", META_RE.sub("", html))
    seen = set()
    for link in LINK_RE.finditer(html):
        tag = link.group(0)
        href = HREF_RE.search(tag)
        if href and REL_STYLESHEET_RE.search(tag):
            target = note(_attr_value(href), html_file.parent)
            if target and target not in seen:
                seen.add(target)
                scan_css(target.read_text(errors="replace"), target.parent, seen)
    for tag in TAG_RE.finditer(html):
        if src := SRC_RE.search(tag.group(0)):
            note(_attr_value(src), html_file.parent)
    scan_css(LINK_RE.sub("", html), html_file.parent, seen)
    return missing


def unrunnable_reason(test_src: str, ref_src: str):
    """Why this reftest cannot probe its own assertion in the capture path.

    These are not engine verdicts. The capture path takes its frame after load
    and does not watch for reftest-wait's completion signal, so comparing those
    frames may measure the wrong moment — upstream WPT would report TIMEOUT
    here, not FAIL. Scoring them as FAIL charges the renderer for the harness's
    missing capability, which is the decorative-instrument class this campaign
    keeps finding (forensics 2026-07-24).
    """
    if "reftest-wait" in test_src or "reftest-wait" in ref_src:
        return "reftest-wait: the capture path does not watch for the completion signal"
    if re.search(r"name\s*=\s*[\"']?fuzzy\b", test_src, re.IGNORECASE):
        return "fuzzy annotation: tolerance matching not implemented"
    return None


def gate(test_src: str, ref_src: str, absent):
    """Why a case stops before any pixels, as a case update; None to go on.

    A missing resource comes first: it is the instrument's fault on any page, and
    a SKIP would hide it for as long as the page stays unscored.
    """
    if absent:
        return {
            "status": "ERROR",
            "reason": f"local resources not on disk {sorted(set(absent))} — "
                      f"add them to MANIFEST.support_paths; pixels not compared",
        }
    unrunnable = unrunnable_reason(test_src, ref_src)
    return {"status": "SKIP", "reason": unrunnable} if unrunnable else None


def webfont_dependent(test_src: str, ref_src: str) -> bool:
    """Does either side declare a web font it needs to render as designed?"""
    for src in (test_src, ref_src):
        if re.search(r"name\s*=\s*[\"']?flags\b[^>]*ahem", src, re.IGNORECASE):
            return True
        if re.search(r"href\s*=\s*[\"']?/fonts/", src, re.IGNORECASE):
            return True
        if "@font-face" in src:
            return True
    return False


def capture_args(html_file: Path, out_ppm: Path, viewport):
    """The parity-capture call. Script stays off unless the page has one, and
    with it on a script that throws fails the capture (an ERROR, not a frame)."""
    cmd = [
        str(CAPTURE_BIN),
        "--html-file", str(html_file),
        "--width", str(viewport["width"]),
        "--height", str(viewport["height"]),
        "--dump-frame", str(out_ppm),
    ]
    if "<script" in html_file.read_text(errors="replace").lower():
        cmd.append("--run-inline-scripts")
    return cmd


def capture(html_file: Path, out_ppm: Path, viewport):
    """Render through the engine. Returns None on success, reason on failure."""
    cmd = capture_args(html_file, out_ppm, viewport)
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


def campaign_diff(test_ppm: Path, ref_ppm: Path):
    """The campaign's pixelmatch on this pair: `(diff_pct, None)`, or `(None, why)`
    when it could not be measured. Reported beside the status, never used to set it."""
    if not (ORACLE_DIR / "node_modules").is_dir():
        return None, "tools/parity_oracle/node_modules is missing"
    try:
        proc = subprocess.run(
            ["node", "wpt_pair_diff.mjs", str(test_ppm), str(ref_ppm)],
            cwd=ORACLE_DIR, capture_output=True, text=True, timeout=60,
        )
    except FileNotFoundError:
        return None, "node not found"
    except subprocess.TimeoutExpired:
        return None, "wpt_pair_diff.mjs timeout (60s)"
    except OSError as e:
        return None, f"node did not start: {type(e).__name__}"
    if proc.returncode != 0:
        tail = (proc.stderr or proc.stdout or "").strip().splitlines()
        return None, f"wpt_pair_diff.mjs exit {proc.returncode}: {tail[-1][:200] if tail else 'no output'}"
    try:
        diff_pct = json.loads(proc.stdout)["diff_pct"]
    except ValueError:
        return None, "wpt_pair_diff.mjs printed no JSON"
    except (KeyError, TypeError):
        return None, "wpt_pair_diff.mjs JSON has no diff_pct"
    if isinstance(diff_pct, bool) or not isinstance(diff_pct, (int, float)) or not math.isfinite(diff_pct):
        return None, "wpt_pair_diff.mjs diff_pct is not a number"
    return round(diff_pct, 4), None


def campaign_counts(cases):
    """Scored cases against the campaign threshold; an unmeasured one is neither."""
    limit = THRESHOLDS["default"]
    scored = [c for c in cases if c["status"] in ("PASS", "FAIL")]
    measured = [c["campaign_diff_pct"] for c in scored if c["campaign_diff_pct"] is not None]
    return {
        "within": sum(1 for d in measured if d <= limit),
        "over": sum(1 for d in measured if d > limit),
        "not_measured": len(scored) - len(measured),
    }


def unproven_passes(cases, entries_by_id):
    """PASSes the manifest does not yet believe, each with its reason."""
    return {
        c["id"]: entries_by_id[c["id"]]["unproven"]
        for c in cases
        if c["status"] == "PASS" and entries_by_id[c["id"]].get("unproven")
    }


def oracle_record():
    """What decided each case: the reftest's own reference, no Chrome involved."""
    return {
        "kind": "reftest: test frame vs reference frame, both through parity-capture",
        "status_rule": {"name": "exact RGB match", "max_diff_pct": WPT_MAX_DIFF_PCT},
        "campaign_rule": {
            "name": "pixelmatch, reported only, never sets a status",
            "pixelmatch_threshold": CAMPAIGN_PIXELMATCH_THRESHOLD,
            "threshold_pct": THRESHOLDS["default"],
        },
    }


def develop_sha_at(cwd: Path):
    """The develop commit this head is on; None without origin/develop, never HEAD."""
    proc = subprocess.run(
        ["git", "merge-base", "HEAD", "origin/develop"],
        cwd=cwd, capture_output=True, text=True,
    )
    return proc.stdout.strip() if proc.returncode == 0 and proc.stdout.strip() else None


def run_pair(test_file: Path, ref_file: Path, viewport, workdir: Path, stage: bool = True):
    """Render both sides and compare. Returns a partial case dict."""
    test_ppm = workdir / "test.ppm"
    ref_ppm = workdir / "ref.ppm"

    if stage:
        test_file = stage_html(test_file)
        ref_file = stage_html(ref_file)

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
    campaign_pct, campaign_why = campaign_diff(test_ppm, ref_ppm)
    # compare_frames is total TODAY (dimension mismatch = full-frame diff),
    # but `diff_pct` is a nullable field everywhere rows round-trip through
    # JSON, and the guard test treats every consumer alike — rightly, since
    # a refactor that routes a loaded row through here would make the
    # ordering below silently compare None. Fail loud instead.
    if diff_pct is None:
        return {
            "status": "ERROR",
            "reason": "comparator returned no diff_pct — refusing to grade",
        }
    status = "PASS" if diff_pct <= WPT_MAX_DIFF_PCT else "FAIL"
    return {
        "status": status,
        "reason": None,
        "diff_pct": round(diff_pct, 4),
        "diff_pixels": diff_pixels,
        "campaign_diff_pct": campaign_pct,
        "campaign_not_measured": campaign_why,
    }


def negative_control(viewport) -> bool:
    """Prove the oracle can go red. True = it failed the mismatched pair."""
    test = NEGATIVE_CONTROL_DIR / "control.html"
    notref = NEGATIVE_CONTROL_DIR / "control-notref.html"
    if not test.exists() or not notref.exists():
        print("HARNESS ERROR: negative-control fixtures missing", file=sys.stderr)
        return False
    with tempfile.TemporaryDirectory(prefix="wpt-negctl-") as td:
        result = run_pair(test, notref, viewport, Path(td), stage=False)
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
    parser.add_argument("--allow-stale", action="store_true",
                        help="skip the binary-freshness guard (records the run as suspect)")
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
              f"run: cargo build --profile parity -p parity-capture", file=sys.stderr)
        return 1

    # n24 lesson (2026-08-12): this runner once scored a stale binary as a
    # confirmed null result. The artifact it consumes must be newer than the
    # sources it was built from, or the run is not a measurement.
    if not args.allow_stale:
        newest_src, newest_path = 0.0, None
        roots = [REPO_ROOT / "crates", REPO_ROOT / "Cargo.lock"]
        for root in roots:
            candidates = root.rglob("*.rs") if root.is_dir() else [root]
            for f in candidates:
                m = f.stat().st_mtime
                if m > newest_src:
                    newest_src, newest_path = m, f
        if CAPTURE_BIN.stat().st_mtime < newest_src:
            print(f"parity-capture is STALE: {newest_path} is newer than the binary.\n"
                  f"run: cargo build --profile parity -p parity-capture\n"
                  f"(or pass --allow-stale to record a knowingly-suspect run)",
                  file=sys.stderr)
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
            "campaign_diff_pct": None,
            "campaign_not_measured": None,
            "rel_match": None,
            "blocked_by": None,
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
                    test_src = test_file.read_text(errors="replace")
                    ref_src = ref_file.read_text(errors="replace")
                    absent = missing_resources(test_file, WPT_ROOT) + missing_resources(ref_file, WPT_ROOT)
                    stop = gate(test_src, ref_src, absent)
                    if stop:
                        case.update(stop)
                    else:
                        if webfont_dependent(test_src, ref_src):
                            case["blocked_by"] = WEBFONT_GAP
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

    # Whether THIS run has the shape the pin calls presumptively dishonest:
    # everything green, nothing red, on the full seed. Computed, not asserted
    # — a field hardcoded to true says the same thing after a 6-FAIL run as
    # after a suspicious one, which makes it decoration rather than a check.
    all_green = (
        counts["PASS"] == len(cases)
        and counts["FAIL"] == 0
        and counts["ERROR"] == 0
    )

    last_run = {
        "schema": 1,
        "wpt_pin": manifest["wpt_pin"],
        "hiwave_git_sha": hiwave_sha,
        "develop_sha": develop_sha_at(REPO_ROOT),
        "oracle": oracle_record(),
        "capture_binary": capture_label(CAPTURE_BIN),
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
        "campaign_rule_counts": campaign_counts(cases),
        "unproven_passes": unproven_passes(cases, {e["id"]: e for e in entries}),
        "attribution": {
            "_comment": "Fails grouped by a named engine capability gap. Attribution only — these cases stay in the denominator. A gap here is a lane, not an excuse.",
            "blocked_fails": {
                gap: sorted(c["id"] for c in cases
                            if c["status"] == "FAIL" and c.get("blocked_by") == gap)
                for gap in sorted({c["blocked_by"] for c in cases
                                   if c["status"] == "FAIL" and c.get("blocked_by")})
            },
            "suspect_passes": {
                "_comment": "PASSes on tests that declare a web font. Before n33 the font never loaded, so these were the dangerous direction (a green case not measuring its own assertion); since n33 the face loads (TTF/OTF) and a PASS here is measured in the declared font. Kept as the same field for trendline continuity.",
                "ids": sorted(c["id"] for c in cases
                              if c["status"] == "PASS" and c.get("blocked_by")),
            },
        },
        "cases": cases,
        "honesty": {
            "all_green": all_green,
            "all_green_suspect": all_green,
            "negative_control": "FAIL as required (checked every run; run aborts otherwise)",
            "note": (
                "SUSPECT: this run is all-green on the full seed, which the W0b "
                "exit criteria treat as presumptively a lying harness rather than "
                "a passing engine. Verify the oracle before quoting this rate."
                if all_green else
                "Not suspect: this run contains red "
                f"({counts['FAIL']} FAIL, {counts['ERROR']} ERROR), so the oracle "
                "demonstrably distinguishes pass from fail on real cases. A run "
                "with pass==n and fail==0 and error==0 would flip this field."
            ),
        },
    }

    if not args.case:
        LAST_RUN_PATH.write_text(json.dumps(last_run, indent=2) + "\n")
        print(f"\nwrote {LAST_RUN_PATH.relative_to(REPO_ROOT)}")

    print(f"\nWPT Tier-1: {counts['PASS']}/{scored} scored "
          f"(pass {counts['PASS']} / fail {counts['FAIL']} / "
          f"skip {counts['SKIP']} / error {counts['ERROR']}, n={len(cases)}) "
          f"@ pin {manifest['wpt_pin'][:12]}")
    held = last_run["campaign_rule_counts"]
    print(f"  campaign rule, reported only: {held['within']} within "
          f"{THRESHOLDS['default']}%, {held['over']} over, {held['not_measured']} not measured")
    for why in sorted({c["campaign_not_measured"] for c in cases if c["campaign_not_measured"]}):
        print(f"  campaign rule not measured: {why}")
    if last_run["unproven_passes"]:
        print(f"  unproven passes: {len(last_run['unproven_passes'])} (see last-run.json)")
    for gap, ids in last_run["attribution"]["blocked_fails"].items():
        print(f"  attributed: {len(ids)} fail(s) blocked by {gap}")
    suspect = last_run["attribution"]["suspect_passes"]["ids"]
    if suspect:
        print(f"  web-font PASS(es) ({len(suspect)}; measured in the declared face since n33): "
              + ", ".join(suspect))
    return 0


if __name__ == "__main__":
    sys.exit(main())
