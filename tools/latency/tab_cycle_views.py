#!/usr/bin/env python3
"""Tab open/close leak check in the real window (CH2.E35).

For each binary: start the app on a throwaway profile restored on a local
fixture page, then run N cycles of

    Cmd+T -> type a local fixture URL in the address bar, Return -> Cmd+W

and read, before the first cycle, after each mark and again some seconds
after the last close:

* live ``RustKitContentView`` objects (the NSView class every RustKit host
  view is: content, chrome, panels), counted on the process heap with
  ``heap(1)``;
* live ``WgpuObserverLayer`` objects (the CAMetalLayer subclass wgpu makes
  for each presenter GPU surface);
* the process RSS (``ps``).

    python3 tools/latency/tab_cycle_views.py --app <hiwave> [--app <hiwave> ...]
        [--cycles 30] [--marks 10,30] [--settle 10] [--out DIR]

It reuses ``tools/real_window/driver.py`` (fixture server, isolated HOME
profile, hwdrive and its HID-safety guard), takes the real-window lock
(``~/.claude/realwindow.lock``) for the whole run and terminates every app
it started. Prints a markdown table and writes ``tab_cycle.json`` to --out.

Exit code: 0 measured, 2 could not run (lock busy, no grants, guard bail).
"""
import argparse
import json
import pathlib
import re
import subprocess
import sys
import tempfile
import time

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "real_window"))
import driver as rw  # noqa: E402

LOCK = pathlib.Path.home() / ".claude" / "realwindow.lock"
PAGE = "e9_final.html"
KEY_T, KEY_W, KEY_RETURN = 17, 13, 36
# The host NSView class, and the CAMetalLayer subclass wgpu makes for each
# surface (heap(1) names it `WgpuObserverLayer@0x<class address>`).
CLASSES = ("RustKitContentView", "WgpuObserverLayer")


def heap_counts(pid, keep=None):
    """Live instances of each class in CLASSES on `pid`'s heap. The full
    heap(1) summary is written to `keep` when given."""
    r = subprocess.run(["/usr/bin/heap", str(pid)], capture_output=True, text=True, timeout=120)
    if keep is not None:
        keep.write_text(r.stdout)
    counts = {c: 0 for c in CLASSES}
    if r.returncode != 0:
        return dict(counts, error=(r.stderr or r.stdout).strip()[-200:])
    counts.update(parse_heap(r.stdout))
    return counts


def parse_heap(text):
    """Instances per class in CLASSES from a heap(1) summary."""
    counts = {c: 0 for c in CLASSES}
    for line in text.splitlines():
        # COUNT  BYTES  AVG  CLASS_NAME  TYPE  BINARY
        parts = line.split()
        if len(parts) >= 6 and parts[0].isdigit() and parts[-2] == "ObjC":
            name = parts[3].split("@0x")[0]
            if name in counts and len(parts) == 6:
                counts[name] += int(parts[0])
    return counts


def rss_mb(pid):
    r = subprocess.run(["ps", "-o", "rss=", "-p", str(pid)], capture_output=True, text=True)
    try:
        return round(int(r.stdout.strip()) / 1024, 1)
    except ValueError:
        return None


class Args:
    def __init__(self, app, out, hwdrive):
        self.app, self.out, self.hwdrive = app, out, hwdrive


def measure(d, label):
    pid = d.app.proc.pid
    row = dict(at=label, rss_mb=rss_mb(pid))
    slug = re.sub(r"[^a-z0-9]+", "-", label.lower()).strip("-")
    row.update(heap_counts(pid, d.out / ("heap-%s.txt" % slug)))
    print("    %-22s views %-4s surfaces %-4s rss %s MB" % (
        label, row["RustKitContentView"], row["WgpuObserverLayer"], row["rss_mb"]), flush=True)
    return row


def one_cycle(d, i):
    """One Cmd+T, typed local URL, Return, Cmd+W. Returns what it saw."""
    pid = d.app.proc.pid
    log_from = len(d.app.log_text())
    d.hw("key", pid, KEY_T, "cmd")
    made = bool(d.app.wait_log(r"New tab - navigating to start page", 5, log_from))
    time.sleep(0.8)
    code, win = d.hw("window", pid)
    # The address bar: right of the sidebar and the nav buttons, in the
    # toolbar row (as driver.py's a2_chrome_typed_url).
    d.hw("click", pid, int(win["w"] * 0.55), 100)
    time.sleep(0.4)
    mark = d.fixture.mark()
    target = "%s/%s?cycle=%d" % (d.fixture.base, PAGE, i)
    d.hw("type", pid, target)
    d.hw("key", pid, KEY_RETURN)
    loaded = d.fixture.wait("/%s?cycle=%d" % (PAGE, i), 10, mark)
    time.sleep(0.8)
    # The chrome closes the tab it shows as active; a Cmd+W that reaches it
    # before its tab list caught up closes nothing, so it is pressed again
    # (at most twice more) until the app logs the close.
    closed = False
    for _ in range(3):
        log_from = len(d.app.log_text())
        d.hw("key", pid, KEY_W, "cmd")
        closed = bool(d.app.wait_log(r"Closed tab", 3, log_from))
        if closed:
            break
    time.sleep(0.6)
    return dict(new_tab=made, loaded=loaded, closed=closed)


def run_app(binary, out, hwdrive, cycles, marks, settle):
    d = rw.Driver(Args(binary, out, hwdrive))
    if not (d.can_input and d.can_capture):
        d.close_page()
        raise SystemExit("seat cannot post input or capture: %s" % json.dumps(d.pre))
    rows, steps = [], []
    try:
        if not d.open_page("tab_cycle", PAGE):
            raise SystemExit("the app did not load %s; see %s" % (PAGE, out))
        time.sleep(3.0)
        rows.append(measure(d, "before"))
        for i in range(1, cycles + 1):
            steps.append(one_cycle(d, i))
            if not d.app.alive():
                raise SystemExit("the app exited during cycle %d" % i)
            if i in marks:
                rows.append(measure(d, "after %d" % i))
        time.sleep(settle)
        rows.append(measure(d, "%d s after last close" % settle))
    finally:
        d.close_page()
    ok = {k: sum(1 for s in steps if s[k]) for k in ("new_tab", "loaded", "closed")}
    return dict(app=str(pathlib.Path(binary).resolve()), rows=rows, cycles=cycles, steps_ok=ok,
                results=d.results)


def table(runs):
    lines = ["| binary | at | host NSViews (RustKitContentView) | GPU surfaces (WgpuObserverLayer) | RSS MB |",
             "|---|---|---|---|---|"]
    for run in runs:
        for row in run["rows"]:
            lines.append("| %s | %s | %s | %s | %s |" % (
                run["name"], row["at"], row["RustKitContentView"], row["WgpuObserverLayer"], row["rss_mb"]))
    lines.append("")
    for run in runs:
        lines.append("%s: %d cycles; Cmd+T answered %d, typed page loaded %d, Cmd+W logged %d" % (
            run["name"], run["cycles"], run["steps_ok"]["new_tab"], run["steps_ok"]["loaded"],
            run["steps_ok"]["closed"]))
    return "\n".join(lines)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--app", action="append", required=True, help="a hiwave binary; repeat to compare")
    ap.add_argument("--name", action="append", help="a label per --app (default: file name)")
    ap.add_argument("--cycles", type=int, default=30)
    ap.add_argument("--marks", default="10,30")
    ap.add_argument("--settle", type=float, default=10.0)
    ap.add_argument("--out", default=str(pathlib.Path(tempfile.gettempdir()) / "hiwave-tab-cycle"),
                    help="logs, profiles and tab_cycle.json (keep it outside the checkout)")
    ap.add_argument("--hwdrive", help="a prebuilt hwdrive (default: compile it into --out)")
    args = ap.parse_args()
    marks = {int(m) for m in args.marks.split(",") if m}
    names = args.name or [pathlib.Path(a).name for a in args.app]
    out = pathlib.Path(args.out).resolve()
    out.mkdir(parents=True, exist_ok=True)
    try:
        LOCK.mkdir()
    except FileExistsError:
        print("real window busy (%s exists)" % LOCK, file=sys.stderr)
        return 2
    runs = []
    try:
        hwdrive = args.hwdrive
        for name, app in zip(names, args.app):
            print("%s: %s" % (name, app), flush=True)
            try:
                run = run_app(app, out / name, hwdrive, args.cycles, marks, args.settle)
            except rw.HidUnsafe as e:
                print("STOP (HID safety): %s" % e, file=sys.stderr)
                return 2
            run["name"] = name
            runs.append(run)
            hwdrive = hwdrive or str(out / name / "hwdrive")
    finally:
        for app in args.app:
            left = subprocess.run(["pgrep", "-f", str(pathlib.Path(app).resolve())],
                                  capture_output=True, text=True).stdout.split()
            if left:
                print("warning: %s still running as %s" % (app, left), file=sys.stderr)
        LOCK.rmdir()
    (out / "tab_cycle.json").write_text(json.dumps(runs, indent=1))
    print()
    print(table(runs))
    return 0


if __name__ == "__main__":
    sys.exit(main())
