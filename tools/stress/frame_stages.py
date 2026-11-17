#!/usr/bin/env python3
"""Per-frame stage split of a stress run (FRAME-STAGES): medians per frame, by stage.

    python3 tools/stress/frame_stages.py --hiwave <run>.work/app.log
    python3 tools/stress/frame_stages.py --chrome <run>.trace.json

HiWave: the `Frame stages ...` lines parity-capture prints per live turn
with `--frame-stages` (`rustkit_engine::frame_stage_line`). A frame is a
turn that rendered; the steady stretch is the longest run of such turns
with no gap over GAP_MS, and the frames read are its middle STEADY share.

Chrome: its own trace (`--trace-startup`, JSON). A frame is the stretch
of the renderer main thread from one `ProxyMain::BeginMainFrame` to the
next; the steady stretch is the longest run with no gap over GAP_MS, its
middle STEADY share read. Each thread's time is self time (an event's
duration less its children's), classed by event name (`chrome_class`);
work on other threads counts in the frame its task started in.

Shapes only: stage names and milliseconds. Prints JSON.
"""
import argparse
import json
import re
import statistics
import sys

GAP_MS = 2000.0
STEADY = (0.2, 0.8)

HIWAVE_STAGES = ["script_ms", "host_ms", "canvas_raster_ms", "build_ms", "layout_ms", "display_list_ms", "snapshot_ms", "svg_ms",
                 "render_ms", "acquire_ms", "execute_ms", "present_ms", "other_ms", "busy_ms", "wait_ms"]
LINE = re.compile(r"Frame stages ((?:[a-z_]+=[-0-9.]+ ?)+)")
# SCRIPT-SPLIT: parity-capture's running host call rows, every 25 turns and at the end:
# `Host calls turn=N rows=name/op:calls:ms,...` (cumulative; names are host functions and operations).
HOST_LINE = re.compile(r"Host calls turn=([0-9]+) rows=([A-Za-z0-9_/?:.,-]*)")


def steady(items, t_of):
    """The middle STEADY share of the longest run of `items` (in time order) with no gap over GAP_MS."""
    best, run = [], []
    for it in items:
        if run and t_of(it) - t_of(run[-1]) > GAP_MS:
            run = []
        run.append(it)
        if len(run) > len(best):
            best = list(run)
    a, b = int(len(best) * STEADY[0]), max(int(len(best) * STEADY[1]), int(len(best) * STEADY[0]) + 1)
    return best[a:b]


def median(values):
    return round(statistics.median(values), 4) if values else None


def hiwave_turns(text):
    turns = []
    for m in LINE.finditer(text):
        fields = dict(kv.split("=", 1) for kv in m.group(1).split())
        turns.append({k: float(v) for k, v in fields.items()})
    return turns


def hiwave_summary(text):
    """Per-frame medians of each stage, and the frame interval (busy + the wait before it)."""
    turns = hiwave_turns(text)
    frames = steady([t for t in turns if t.get("renders", 0) > 0], lambda t: t["t_ms"])
    out = {"engine": "hiwave-headless", "turns": len(turns), "frames": len(frames),
           "stages": {k: median([f[k] for f in frames]) for k in HIWAVE_STAGES}}
    starts = [f["t_ms"] for f in frames]
    out["frame_interval_ms"] = median([b - a for a, b in zip(starts, starts[1:])])
    out["script_less_raster_ms"] = median([f["script_ms"] - f["canvas_raster_ms"] for f in frames])
    out["renders_per_frame"] = median([f["renders"] for f in frames])
    out["canvas_draws_per_frame"] = median([f["canvas_draws"] for f in frames])
    if frames and "host_ms" in frames[0]:
        # SCRIPT-SPLIT: script = the engine's JS execution + the host calls it made (canvas raster inside).
        out["js_ms"] = median([f["script_ms"] - f["host_ms"] for f in frames])
        out["host_less_raster_ms"] = median([f["host_ms"] - f["canvas_raster_ms"] for f in frames])
        out["host_calls_per_frame"] = median([f["host_calls"] for f in frames])
        out["host_rows_per_frame"] = host_rows(text, frames)
    return out


def host_row_blocks(text):
    """The `Host calls` lines: [(turn, {(name, op): (calls, ms)})] in log order."""
    blocks = []
    for m in HOST_LINE.finditer(text):
        rows = {}
        for item in filter(None, m.group(2).split(",")):
            key, calls, ms = item.rsplit(":", 2)
            name, _, op = key.partition("/")
            rows[(name, op)] = (int(calls), float(ms))
        blocks.append((int(m.group(1)), rows))
    return blocks


def host_rows(text, frames):
    """Per-frame host call rows over the steady frames: the difference between the last `Host calls`
    line at or before the first steady frame's turn and the first at or after the last one's, over the
    frames rendered between them. Rows by time, largest first: name, op, calls and ms per frame."""
    blocks = host_row_blocks(text)
    if not frames or not blocks:
        return []
    first, last = frames[0]["turn"], frames[-1]["turn"]
    before = [b for b in blocks if b[0] <= first]
    if not before:
        return []
    t0, r0 = before[-1]
    # The first line at or after the last steady frame; a run cut short has none, then its last line.
    after = [b for b in blocks if b[0] >= last] or [b for b in blocks if b[0] > t0][-1:]
    if not after:
        return []
    t1, r1 = after[0]
    rendered = sum(1 for t in hiwave_turns(text) if t0 < t["turn"] <= t1 and t.get("renders", 0) > 0)
    if rendered == 0:
        return []
    out = []
    for key, (calls, ms) in r1.items():
        c0, m0 = r0.get(key, (0, 0.0))
        if calls > c0:
            out.append({"name": key[0], "op": key[1], "calls": round((calls - c0) / rendered, 2),
                        "ms": round((ms - m0) / rendered, 4),
                        "us_per_call": round(1000.0 * (ms - m0) / (calls - c0), 3)})
    out.sort(key=lambda r: -r["ms"])
    return out


# Chrome event names to a stage, first match wins (self time of the event).
CHROME_MAIN = [
    ("script", re.compile(r"^(FunctionCall|FireAnimationFrame|TimerFire|EvaluateScript|EventDispatch|RunMicrotasks|"
                          r"v8\.|V8\.|MajorGC|MinorGC|BlinkGC|ThreadState::|CppGC|v8::)")),
    ("style", re.compile(r"(UpdateLayoutTree|RecalculateStyles|Style)")),
    ("layout", re.compile(r"^(Layout|Blink\.Layout\.UpdateTime|LayoutView|LocalFrameView::PerformLayout)")),
    ("paint", re.compile(r"(Paint|Layerize|CompositingInputs|UpdateLayer|PaintArtifactCompositor|RasterInvalidation)")),
]


def chrome_class_main(name):
    for stage, pat in CHROME_MAIN:
        if pat.search(name):
            return stage
    return "main_other"


def chrome_thread_class(thread):
    """What a non-main thread's time is, or None to leave it out."""
    if thread == "Compositor":
        return "compositor"
    if thread.startswith("CompositorTileWorker"):
        return "raster_workers"
    if thread == "CrGpuMain":
        return "gpu_main"
    if thread in ("VizCompositorThread", "CompositorGpuThread"):
        return "display"
    return None


def self_times(events):
    """(start_us, name, self_us, depth) for each complete event of one thread (sorted by start)."""
    events = sorted(events, key=lambda e: (e["ts"], -e.get("dur", 0)))
    out, stack = [], []  # stack: [end, index]
    for e in events:
        ts, dur = e["ts"], e.get("dur", 0)
        while stack and stack[-1][0] <= ts:
            stack.pop()
        idx = len(out)
        out.append([ts, e["name"], dur, len(stack)])
        if stack:
            out[stack[-1][1]][2] -= dur
        stack.append([ts + dur, idx])
    return out


def chrome_summary(trace):
    ev = trace["traceEvents"] if isinstance(trace, dict) else trace
    names = {}
    for e in ev:
        if e.get("ph") == "M" and e.get("name") == "thread_name":
            names[(e["pid"], e["tid"])] = e["args"]["name"]
    by_thread = {}
    for e in ev:
        if e.get("ph") == "X" and "dur" in e:
            by_thread.setdefault((e["pid"], e["tid"]), []).append(e)
    # The renderer main thread with the most frames.
    mains = [(sum(1 for e in evs if e["name"] == "ProxyMain::BeginMainFrame"), key)
             for key, evs in by_thread.items() if names.get(key) == "CrRendererMain"]
    if not mains:
        return {"engine": "chrome", "error": "no renderer main thread in the trace"}
    _, main_key = max(mains)
    renderer = main_key[0]
    bmf = sorted(e["ts"] for e in by_thread[main_key] if e["name"] == "ProxyMain::BeginMainFrame")
    frames = steady([t / 1000.0 for t in bmf], lambda t: t)
    if len(frames) < 2:
        return {"engine": "chrome", "error": "fewer than two steady frames", "frames": len(frames)}
    edges = [f * 1000.0 for f in frames]
    n = len(edges) - 1
    per = [dict() for _ in range(n)]

    def slot(ts):
        if ts < edges[0] or ts >= edges[-1]:
            return None
        lo, hi = 0, n
        while hi - lo > 1:
            mid = (lo + hi) // 2
            if edges[mid] <= ts:
                lo = mid
            else:
                hi = mid
        return lo

    for key, evs in by_thread.items():
        thread = names.get(key, "?")
        if key == main_key:
            for ts, name, self_us, _ in self_times(evs):
                i = slot(ts)
                if i is not None:
                    c = chrome_class_main(name)
                    per[i][c] = per[i].get(c, 0.0) + self_us / 1000.0
            continue
        cls = chrome_thread_class(thread)
        if cls is None or (cls in ("compositor", "raster_workers") and key[0] != renderer):
            continue
        for ts, dur in task_totals(evs):  # each task's whole time, counted once
            i = slot(ts)
            if i is not None:
                per[i][cls] = per[i].get(cls, 0.0) + dur / 1000.0
    stages = ["script", "style", "layout", "paint", "main_other", "compositor", "raster_workers", "gpu_main", "display"]
    out = {"engine": "chrome", "frames": n,
           "stages": {s: median([p.get(s, 0.0) for p in per]) for s in stages},
           "frame_interval_ms": median([(b - a) / 1000.0 for a, b in zip(edges, edges[1:])])}
    out["main_busy_ms"] = median([sum(p.get(s, 0.0) for s in stages[:5]) for p in per])
    return out


def task_totals(events):
    """Whole duration of each outermost event of one thread: (start_us, dur_us)."""
    events = sorted(events, key=lambda e: (e["ts"], -e.get("dur", 0)))
    out, end = [], -1
    for e in events:
        if e["ts"] >= end:
            out.append((e["ts"], e.get("dur", 0)))
            end = e["ts"] + e.get("dur", 0)
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    g = ap.add_mutually_exclusive_group(required=True)
    g.add_argument("--hiwave", help="parity-capture log with Frame stages lines")
    g.add_argument("--chrome", help="Chrome trace JSON")
    args = ap.parse_args()
    if args.hiwave:
        with open(args.hiwave, errors="replace") as f:
            out = hiwave_summary(f.read())
    else:
        with open(args.chrome) as f:
            out = chrome_summary(json.load(f))
    json.dump(out, sys.stdout, indent=1)
    print()


if __name__ == "__main__":
    main()
