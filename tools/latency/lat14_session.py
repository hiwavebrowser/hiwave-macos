#!/usr/bin/env python3
"""LAT14 proving session: does an animation layer outlive its document?

Page A (`pages/lat14_a.html`) runs `element.animate()` on six boxes for a few
seconds, then sends the same tab to page B (`pages/lat14_b.html`): some text
and one opacity-only CSS animation; after --b-seconds B goes to page C, so the
app logs B's relayout summary (frames painted, layouts skipped). Before LAT14 the view's animated-style
layer kept page A's entries, and every animation frame of page B ran a full
relayout. This runs that sequence in the real HiWave window for each binary
given (A,B,A,B with --runs 2) and reports, for page B only:

  - `relayout cause=animation` lines of B's view, their mean took_ms and the
    `full_reason` values;
  - the content thread's hold p95 for B's view (the app's every-100 summary;
    the first summary after B's load is dropped, it can hold page A's turns).

    python3 tools/latency/lat14_session.py --app develop=<bin> --app candidate=<bin>
                                           [--runs 2] [--b-seconds 15] [--out DIR]

The app runs as Pete runs it: content thread on by the app's default
(HIWAVE_CONTENT_THREAD unset), default log level, no test hooks, a throwaway
profile (HOME is set for the app only), the Inspector closed. No input is
posted: page A navigates by script. It takes the real-window lock
(~/.claude/realwindow.lock) and will not start unless the Mac has been idle
for --min-idle seconds. Every app it starts is terminated before it exits.
"""
import argparse
import json
import os
import pathlib
import re
import shutil
import statistics
import subprocess
import sys
import time
from collections import Counter

sys.dont_write_bytecode = True  # no __pycache__ in the checkout
HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "real_window"))
import driver  # noqa: E402  (Fixture, App, set_tab_urls)

driver.PAGES = HERE / "pages"
LOCK = pathlib.Path.home() / ".claude" / "realwindow.lock"

VIEW_LOAD = re.compile(r"Loading URL id=EngineViewId\((\d+)\) url=(\S+)")
RELAYOUT = re.compile(r"id=EngineViewId\((\d+)\)\}: rustkit_engine: relayout cause=(\S+) .*?took_ms=([\d.]+)")
FULL_REASON = re.compile(r"full_reason=(\S+)")
ANIM_MISS = re.compile(r'anim_miss="([^"]*)"')
HOLDS = re.compile(r"Content thread holds view=(\d+) .*?n=(\d+) p50_ms=([\d.]+) p95_ms=([\d.]+) max_ms=([\d.]+)")


def idle_seconds():
    r = subprocess.run(["ioreg", "-c", "IOHIDSystem"], capture_output=True, text=True)
    m = re.search(r'"HIDIdleTime" = (\d+)', r.stdout)
    return int(m.group(1)) / 1e9 if m else 0.0


def seed(binary, profile, log):
    """Let the app write its own first-run workspace state."""
    app = driver.App(binary, profile, log, ENV).start()
    end = time.time() + 20
    while time.time() < end and app.alive() and not app.state_file.is_file():
        time.sleep(0.1)
    time.sleep(0.5)
    app.stop()
    if not app.state_file.is_file():
        sys.exit("the app wrote no workspace_state.json; see %s" % log)
    return app.state_file.read_text()


def one_run(label, binary, out, fixture, b_seconds, dwell):
    profile = out / ("profile-" + label)
    shutil.rmtree(profile, ignore_errors=True)
    profile.mkdir(parents=True)
    template = json.loads(seed(binary, profile, out / ("seed-" + label + ".log")))
    driver.set_tab_urls(template, "%s/lat14_a.html?dwell=%d&leave=%d" % (fixture.base, dwell, b_seconds * 1000))
    log = out / (label + ".app.log")
    app = driver.App(binary, profile, log, ENV)
    app.state_file.write_text(json.dumps(template))
    since = fixture.mark()
    app.start()
    try:
        if not fixture.wait("lat14_a_animating", 40, since):
            return {"error": "page A never reported its animations"}
        if not fixture.wait("lat14_b_loaded", 30, since):
            return {"error": "page B never loaded"}
        if not fixture.wait("lat14_c_loaded", b_seconds + 30, since):
            return {"error": "page B never left for page C"}
        time.sleep(1.0)  # B's relayout summary
    finally:
        app.stop()
    return analyse(app.log_text())


def analyse(text):
    lines = text.splitlines()
    b_at, c_at, view = None, None, None
    for i, line in enumerate(lines):
        m = VIEW_LOAD.search(line)
        if m and m.group(2).split("?")[0].endswith("/lat14_b.html"):
            b_at, view = i, m.group(1)
        if m and b_at is not None and m.group(1) == view and m.group(2).endswith("/lat14_c.html"):
            c_at = i
    if b_at is None or c_at is None:
        return {"error": "no 'Loading URL' line for page B or page C"}
    # The summary of page B is logged when the view leaves it, as C loads.
    summary = {}
    for line in lines[c_at:]:
        if "relayout summary url=" in line and "/lat14_b.html" in line:
            for key in ("relayouts", "layouts_skipped", "animation_frames_painted"):
                m = re.search(r"\b%s=(\d+)" % key, line)
                if m:
                    summary[key] = int(m.group(1))
            break
    took, reasons, misses, p95s = [], Counter(), Counter(), []
    for line in lines[b_at:c_at]:
        m = RELAYOUT.search(line)
        if m and m.group(1) == view and m.group(2) == "animation":
            took.append(float(m.group(3)))
            r = FULL_REASON.search(line)
            reasons[r.group(1) if r else "-"] += 1
            a = ANIM_MISS.search(line)
            if a:
                misses[a.group(1)] += 1
        h = HOLDS.search(line)
        if h and h.group(1) == view:
            p95s.append(float(h.group(4)))
    return {
        "view": int(view),
        "anim_relayouts": len(took),
        "mean_took_ms": round(statistics.mean(took), 1) if took else None,
        "full_reason": dict(reasons),
        "anim_miss": dict(misses),
        "hold_p95_ms": p95s[1:],  # the first summary can span page A
        "hold_p95_median_ms": round(statistics.median(p95s[1:]), 1) if len(p95s) > 1 else None,
        "b_summary": summary,
    }


def main():
    global ENV
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--app", action="append", required=True, help="label=path to a hiwave binary (repeat)")
    ap.add_argument("--runs", type=int, default=2, help="rounds over the binaries, interleaved")
    ap.add_argument("--b-seconds", type=float, default=15)
    ap.add_argument("--dwell-ms", type=int, default=4000, help="time on page A before it navigates")
    ap.add_argument("--min-idle", type=float, default=60, help="seconds the Mac must have been idle")
    ap.add_argument("--out", default="lat14-session-out")
    args = ap.parse_args()
    apps = []
    for spec in args.app:
        label, _, path = spec.partition("=")
        if not path:
            label, path = pathlib.Path(spec).name, spec
        apps.append((label, pathlib.Path(path).resolve()))
    out = pathlib.Path(args.out).resolve()
    out.mkdir(parents=True, exist_ok=True)

    # As Pete runs the app: no content-thread override, default log level.
    for k in ("HIWAVE_CONTENT_THREAD", "RUST_LOG", "HIWAVE_DEBUG"):
        os.environ.pop(k, None)
    ENV = {"HIWAVE_TEST_HOOKS": "0"}

    idle = idle_seconds()
    if idle < args.min_idle:
        sys.exit("someone is using this Mac (idle %.0f s < %.0f s); not starting" % (idle, args.min_idle))
    try:
        LOCK.mkdir()
    except FileExistsError:
        sys.exit("real window busy (%s exists)" % LOCK)
    results = []
    try:
        fixture = driver.Fixture()
        for rnd in range(args.runs):
            for label, binary in apps:
                if idle_seconds() + 1 < min(args.min_idle, 20):
                    raise SystemExit("someone started using the Mac; stopping")
                tag = "%s-%d" % (label, rnd + 1)
                print("run %s ..." % tag, flush=True)
                r = one_run(tag, binary, out, fixture, args.b_seconds, args.dwell_ms)
                r["run"] = tag
                results.append(r)
                print("  " + json.dumps(r), flush=True)
    finally:
        LOCK.rmdir()
    (out / "result.json").write_text(json.dumps(results, indent=1))
    print("\n| run | B anim relayouts | mean took_ms | full_reason | hold p95 (median) ms | B frames painted / layouts skipped |")
    print("|---|---|---|---|---|---|")
    for r in results:
        if "error" in r:
            print("| %s | %s | | | | |" % (r["run"], r["error"]))
            continue
        bs = r["b_summary"]
        print("| %s | %d | %s | %s | %s | %s / %s |" % (
            r["run"], r["anim_relayouts"], r["mean_took_ms"] if r["mean_took_ms"] is not None else "-",
            ", ".join("%s %d" % kv for kv in r["full_reason"].items()) or "-",
            r["hold_p95_median_ms"] if r["hold_p95_median_ms"] is not None else "-",
            bs.get("animation_frames_painted", "-"), bs.get("layouts_skipped", "-")))


ENV = {}
if __name__ == "__main__":
    main()
