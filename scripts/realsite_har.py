#!/usr/bin/env python3
"""QB-ARCHIVE-INPUT: keep what each real-site board load received, and replay it offline.

The board (scripts/realsite_board.py) records both sides of every site: RustKit's load with
`parity-capture --dump-har` and the Chrome oracle's capture `a` with Playwright's recordHar
(tools/parity_oracle/realsite.mjs). `archive_inputs` turns each raw recording into:

  <side>.har.gz          the HAR, gzipped, with image/video/audio bodies left out (the entry stays,
                         `content._skipped: "media"`; REALSITE_HAR_KEEP_IMAGES=1 keeps images) and
                         the side's bodies capped (default 50 MB,
                         REALSITE_HAR_CAP_MB): past the cap the largest bodies go first
                         (`_skipped: "cap"`)
  <side>-document.html   the main document that side received (after redirects)

and returns what it kept for the site record (`rec["inputs"][side]`). REALSITE_ARCHIVE_INPUTS=0
turns the recording off.

Replay (bisecting a lost point offline, no live load):

  python3 scripts/realsite_har.py replay --har RUN/SITE/rustkit.har.gz \\
      --capture-bin BIN --out DIR [--url URL] [--server tools/replay/server.py]

starts the loopback HAR replay server on the archived HAR (the handtest one,
~/Repos/.worktrees/handtest-runs/replay-tools-727/tools/replay/server.py, or REALSITE_REPLAY_SERVER)
and runs BIN with the board's arguments and `--replay-proxy`, writing DIR/rustkit.ppm,
DIR/rustkit-display-list.json, DIR/misses.txt and DIR/replay.json. Run it once per candidate
binary and compare the frames (or `node tools/parity_oracle/realsite.mjs diff` against the
archived chrome.png). Requests the HAR does not hold are answered 404 and listed in misses.txt;
media bodies were not archived, so images replay as misses. The replay starts at the main
document the recorded redirects led to (`--url` overrides it): the capture follows a redirect
itself, straight to its target rather than through the replay proxy.
"""

import argparse
import base64
import gzip
import json
import os
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path

DEFAULT_CAP_MB = 50
MEDIA_PREFIXES = ("image/", "video/", "audio/")
DEFAULT_SERVER = (
    Path.home() / "Repos/.worktrees/handtest-runs/replay-tools-727/tools/replay/server.py"
)


def archive_enabled():
    return os.environ.get("REALSITE_ARCHIVE_INPUTS", "1") != "0"


def keep_images():
    """REALSITE_HAR_KEEP_IMAGES=1: keep image bodies (within the cap); video and audio never."""
    return os.environ.get("REALSITE_HAR_KEEP_IMAGES", "0") == "1"


def cap_bytes():
    try:
        return int(float(os.environ.get("REALSITE_HAR_CAP_MB", DEFAULT_CAP_MB)) * 2**20)
    except ValueError:
        return DEFAULT_CAP_MB * 2**20


def _mime(entry):
    return ((entry.get("response") or {}).get("content") or {}).get("mimeType") or ""


def _body_len(content):
    text = content.get("text")
    if text is None:
        return 0
    return len(text.encode("utf-8")) if content.get("encoding") != "base64" else len(text) * 3 // 4


def slim_har(har, cap_bytes, keep_images=False):
    """Leave media bodies out and cap the rest; returns (har, stats). Every entry stays."""
    entries = (har.get("log") or {}).get("entries") or []
    stats = {"entries": len(entries), "skipped_media": 0, "skipped_cap": 0, "capped": False}
    kept = []
    for e in entries:
        content = (e.get("response") or {}).setdefault("content", {})
        mime = _mime(e).lower()
        media = mime.startswith(MEDIA_PREFIXES) and not (keep_images and mime.startswith("image/"))
        if "text" in content and media:
            content.pop("text", None)
            content.pop("encoding", None)
            content["_skipped"] = "media"
            stats["skipped_media"] += 1
        elif "text" in content:
            kept.append(content)
    total = sum(_body_len(c) for c in kept)
    for content in sorted(kept, key=_body_len, reverse=True):
        if total <= cap_bytes:
            break
        total -= _body_len(content)
        content.pop("text", None)
        content.pop("encoding", None)
        content["_skipped"] = "cap"
        stats["skipped_cap"] += 1
        stats["capped"] = True
    stats["body_bytes"] = total
    return har, stats


def main_document(har):
    """(url, text) of the first 2xx HTML answer (redirect hops are passed over), or None."""
    for e in (har.get("log") or {}).get("entries") or []:
        resp = e.get("response") or {}
        status = resp.get("status") or 0
        if not (200 <= status < 300) or "html" not in _mime(e).lower():
            continue
        content = resp.get("content") or {}
        text = content.get("text")
        if text is None:
            continue
        if content.get("encoding") == "base64":
            text = base64.b64decode(text).decode("utf-8", errors="replace")
        return (e.get("request") or {}).get("url"), text
    return None


def archive_inputs(site_dir, side, raw_har, cap_bytes, keep_images=None):
    """Slim and gzip `raw_har` into SITE/<side>.har.gz, write SITE/<side>-document.html, remove the
    raw file; returns what was kept (or `{"error": ...}`)."""
    site_dir, raw_har = Path(site_dir), Path(raw_har)
    if not raw_har.exists():
        return {"error": "no recording"}
    try:
        har = json.loads(raw_har.read_text(encoding="utf-8", errors="replace"))
    except (OSError, json.JSONDecodeError) as e:
        raw_har.unlink(missing_ok=True)
        return {"error": "unreadable recording: %s" % str(e)[:120]}
    if keep_images is None:
        keep_images = globals()["keep_images"]()
    har, stats = slim_har(har, cap_bytes, keep_images)
    out = site_dir / ("%s.har.gz" % side)
    with gzip.open(out, "wt", encoding="utf-8", compresslevel=6) as f:
        json.dump(har, f, separators=(",", ":"))
    stats["har"] = out.name
    stats["har_gz_bytes"] = out.stat().st_size
    doc = main_document(har)
    if doc:
        (site_dir / ("%s-document.html" % side)).write_text(doc[1], encoding="utf-8")
        stats["document"] = doc[0]
    raw_har.unlink(missing_ok=True)
    return stats


# ---------------------------------------------------------------- replay


def replay_site(har_gz, url, capture_bin, out_dir, server=None, width=1280, height=800,
                timeout_ms=30000, extra_args=()):
    """Load `url` with `capture_bin` through the loopback replay server serving `har_gz`."""
    out = Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)
    server = Path(server or os.environ.get("REALSITE_REPLAY_SERVER") or DEFAULT_SERVER)
    if not server.exists():
        return {"status": "error", "error": "no replay server at %s" % server}
    with tempfile.TemporaryDirectory() as td:
        har_path = Path(td) / "site.har"
        opener = gzip.open if str(har_gz).endswith(".gz") else open
        with opener(har_gz, "rt", encoding="utf-8") as src:
            text = src.read()
        har_path.write_text(text, encoding="utf-8")
        # The capture's own redirect hops go straight to their target, not through the replay
        # proxy, so a replay starts at the document the redirects led to.
        if not url:
            doc = main_document(json.loads(text))
            if not doc:
                return {"status": "error", "error": "no HTML document in the HAR to start from"}
            url = doc[0]
        misses = out / "misses.txt"
        server_log = open(out / "server.log", "w")
        srv = subprocess.Popen(
            [sys.executable, str(server), str(har_path), "--port", "0", "--time-scale", "0",
             "--miss-log", str(misses), "--no-tls"],
            stdout=subprocess.PIPE, stderr=server_log, text=True,
            cwd=str(server.parent),
        )
        try:
            line = srv.stdout.readline().strip()
            m = re.search(r"(http://127\.0\.0\.1:\d+)", line)
            if not m:
                return {"status": "error", "error": "replay server did not start: %r" % line}
            proxy = m.group(1)
            frame, dl = out / "rustkit.ppm", out / "rustkit-display-list.json"
            cmd = [str(capture_bin), "--url", url, "--replay-proxy", proxy, "--width", str(width),
                   "--height", str(height), "--timeout-ms", str(timeout_ms), "--dump-frame", str(frame),
                   "--dump-display-list", str(dl), *extra_args]
            started = time.time()
            try:
                p = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout_ms / 1000 + 15)
                lines = [l for l in p.stdout.splitlines() if l.strip().startswith("{")]
                capture = json.loads(lines[-1]) if lines else {"status": "crash", "stderr": p.stderr[-500:]}
            except subprocess.TimeoutExpired:
                capture = {"status": "timeout"}
        finally:
            srv.terminate()
            try:
                srv.wait(5)
            except subprocess.TimeoutExpired:
                srv.kill()
                srv.wait(5)
            srv.stdout.close()
            server_log.close()
    missed = [l for l in misses.read_text().splitlines() if l.strip()] if misses.exists() else []
    res = {"status": capture.get("status", "error"), "proxy": proxy, "misses": len(missed),
           "wall_s": round(time.time() - started, 1), "capture": capture}
    (out / "replay.json").write_text(json.dumps(res, indent=1), encoding="utf-8")
    return res


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("replay", help="load a site from its archived HAR through the replay server")
    r.add_argument("--har", required=True, help="RUN/SITE/rustkit.har.gz (or chrome.har.gz)")
    r.add_argument("--url", help="default: the main document the HAR's redirects led to "
                                    "(redirect hops do not go through the replay proxy)")
    r.add_argument("--capture-bin", required=True)
    r.add_argument("--out", required=True)
    r.add_argument("--server", help="tools/replay/server.py (default: REALSITE_REPLAY_SERVER or handtest's)")
    r.add_argument("--width", type=int, default=1280)
    r.add_argument("--height", type=int, default=800)
    r.add_argument("--timeout-ms", type=int, default=30000)
    a = ap.parse_args()
    res = replay_site(a.har, a.url, a.capture_bin, a.out, a.server, a.width, a.height, a.timeout_ms)
    print(json.dumps({k: v for k, v in res.items() if k != "capture"}))
    return 0 if res["status"] == "ok" else 1


if __name__ == "__main__":
    sys.exit(main())
