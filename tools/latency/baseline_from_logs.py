#!/usr/bin/env python3
"""Latency baseline from HiWave logs (trench row LAT0).

Reads one or more HiWave logs -- Pete's hand-test logs
(``~/Library/Logs/hiwave/live-*.log``) or headless capture stderr
(``x-*/<site>-<n>.stderr.txt``) -- and prints, per site (hostname only),
the latency-relevant counts the engine already logs:

* ``Script stopped at the script budget`` lines by event source, split into
  *leaving* stops (input refused by a document the window has asked to
  leave: elapsed 0, between a navigation signal and the next document's
  first sign of life -- see ``leaving_input_tests.rs``, fixed by #892) and
  *live* stops (everything else);
* ``relayout cause=`` lines by cause, with ``took_ms`` and stage ms;
* ``Forced layouts in the script task`` (count of forced layouts, and their
  ``ms``);
* ``Content thread hold`` / ``Window thread hold`` lines by entry, with
  ``ms`` and the build/layout/display_list/snapshot stages (only holds of
  50 ms or more are logged: ``wgate::HOLD_THRESHOLD_MS``);
* ``Image over the subresource budget`` drops;
* timer pressure: ``relayout cause=timer`` per second of dwell, and the
  ``Page lifecycle step done source="timers"`` callbacks;
* ``Page scripts ran`` and ``Navigation finished`` elapsed_ms;
* ``Input to present`` lines: (LAT1c) ``Input to present view=N host=H n=...
  by_kind=... over_100_ms=... no_frame=... role=page|chrome at=...``, one per
  visit and one per 100 inputs, from each input's native event time to the
  present of the first changed frame after its turn; grouped by the line's
  own view and host (``role=chrome`` lines -- the RustKit chrome and panels --
  under ``(chrome)``), they give the exact p95 per host over every headline
  input (keys, presses, releases, wheels, edits; moves are sampled and kept
  apart) whenever it is 100 ms or more, and "under 100" when it is not. The
  older untagged ``Input to present p50_ms=...`` windows are still read
  (worst window) when a site has no LAT1c line;
* (LAT1) the ``Content thread holds view=N host=H n=... by_entry=...``
  summary lines, one per 100 entries and one per content visit, over
  **every** content-thread entry whatever its length; merged with the hold
  lines (each entry of 50 ms or more) they give the exact hold p95 over all
  entries when it is 50 ms or more, and "under 50" when it is not. Since
  LAT1b a line names its view and host, and it is grouped by those, not by
  where it falls in the log. ``Window thread holds`` lines (written before
  LAT1b by the window thread: a headless capture's load and render, or the
  flag-off page) are no content visit and are ignored; a site with no
  content-thread summary gets n/a, never a verdict;
* the exit metric of ``trench/latency/BASELINE.md`` per site: live
  script-budget stops per minute <= 1, content-thread hold p95 over all
  entries <= 50 ms, and input-to-present p95 <= 100 ms (LAT1c: exact over
  every input of the site; before LAT1c, the worst 100-input window);
* an input-to-paint proxy: for each pointer/mouse event that hit the budget
  line, the ms of the first hold that followed within 2 s.

Nothing it prints carries a path, a query string or a script URL: sites are
hostnames, script sources are reduced to ``script``/``inline``.

Usage::

    tools/latency/baseline_from_logs.py LOG [LOG ...] [--json] [--views]

``--views`` splits every count by engine view id as well (with the RustKit
chrome flag on, the chrome is its own view; the content view loads the
sites). Standard library only.
"""

from __future__ import annotations

import argparse
import json
import math
import re
import sys
from collections import defaultdict
from datetime import datetime

ANSI = re.compile(r"\x1b\[[0-9;]*m")
TS = re.compile(r"^(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d+)Z\s+([A-Z]+)\s+(.*)$")
VIEW = re.compile(r"EngineViewId\((\d+)\)")
KV = re.compile(r"(\w+)=(\"[^\"]*\"|\S+)")
HOST = re.compile(r"url=(?:https?|hiwave)://([^/\s?#:]+)")

POINTER = (
    "pointermove", "mousemove", "pointerover", "pointerout", "pointerenter",
    "pointerleave", "mouseover", "mouseout", "mouseenter", "mouseleave",
)
STAGES = ("build_ms", "layout_ms", "display_list_ms", "snapshot_ms")
# A document is "alive" again once one of these follows a navigation signal.
ALIVE = (
    "Running page script", "Running module script", "Running inline script",
    "Navigation finished", "relayout cause=initial_load", "Page scripts ran",
)
NAV_SIGNALS = (
    "Navigating content to", "Navigating to new active tab", "Reloading",
    "Loading HTML content", "Navigation cancelled", "Loading URL id=",
    '"cmd":"close_tab"',
)
PROXY_WINDOW_S = 2.0
HOLD_THRESHOLD_MS = 50.0  # wgate::HOLD_THRESHOLD_MS: shorter holds get no line of their own
EXIT_STOPS_PER_MIN = 1.0
EXIT_HOLD_P95_MS = 50.0
EXIT_INPUT_P95_MS = 100.0
HOLD_SUMMARY = re.compile(r"\b(Content|Window) thread holds (?:view=(\S+) host=(\S+) )?n=")
INPUT_SUMMARY = re.compile(r"\bInput to present view=(\S+) host=(\S+) n=")
CHROME = "(chrome)"


def pct(values, p):
    """Nearest-rank percentile; None for an empty list."""
    if not values:
        return None
    v = sorted(values)
    k = max(0, min(len(v) - 1, int(round(p / 100.0 * len(v) + 0.5)) - 1))
    return v[k]


def dist(values):
    values = [float(x) for x in values]
    if not values:
        return {"n": 0}
    return {
        "n": len(values),
        "p50": round(pct(values, 50), 1),
        "p95": round(pct(values, 95), 1),
        "max": round(max(values), 1),
        "sum": round(sum(values), 1),
    }


def kv(text):
    out = {}
    for k, v in KV.findall(text):
        out[k] = v.strip('"')
    return out


def num(d, key, default=0.0):
    try:
        return float(d.get(key, default))
    except (TypeError, ValueError):
        return default


def stop_source(raw):
    raw = raw.strip('"')
    if raw.startswith("event:"):
        return raw
    if raw.startswith("inline"):
        return "inline"
    if raw in ("timers", "network"):
        return raw
    return "script"


class Site:
    def __init__(self):
        self.visits = 0
        self.dwell_s = 0.0
        self.stops = defaultdict(lambda: {"leaving": [], "live": [], "late": []})
        self.relayouts = defaultdict(lambda: {
            "took": [], "unstaged": [], "extract": [], "reasons": defaultdict(int), **{s: [] for s in STAGES}})
        self.forced_layouts = 0
        self.forced_tasks = 0
        self.forced_ms = 0.0
        # LAT1: per thread label, entries counted by the summary lines, and
        # the logged holds (>= 50 ms) as (entry, ms).
        self.hold_summary_n = defaultdict(lambda: defaultdict(int))
        # LAT1b: the entries of 50 ms or more the tagged summary lines list
        # themselves, and how many entries those lines covered.
        self.hold_summary_over = []
        self.hold_summary_tagged_n = 0
        self.hold_logged = defaultdict(list)
        self.holds = defaultdict(lambda: {
            "ms": [], **{s: [] for s in STAGES}, "script_ms": [], "other_ms": [], "leaving_ms": []})
        self.image_drops = 0
        self.timer_callbacks = 0
        self.page_scripts_ran = []
        self.nav_finished = []
        self.input_to_present = []  # (p50, p95, max, n)
        # LAT1c: the tagged lines' headline inputs, the ones of 100 ms or
        # more they list, inputs by kind, and inputs that drew no frame.
        self.input_n = 0
        self.input_over = []
        self.input_by_kind = defaultdict(int)
        self.input_no_frame = 0
        self.input_lines = 0
        self.proxy = []  # hold ms following a pointer stop
        self.proxy_entries = defaultdict(int)
        self.watchdog = []  # ran_ms of watchdog interrupts


def parse(paths, by_view=False):
    sites = defaultdict(Site)
    for path in paths:
        parse_one(path, sites, by_view)
    return sites


def parse_one(path, sites, by_view):
    loading = {}        # view -> host being loaded
    committed = {}      # view -> host of the live document
    leaving = set()     # views with a navigation signal and no new document yet
    visit_start = {}    # view -> (host, ts) of the current visit
    last_ts = None
    content_view = None
    pending_proxy = []  # (ts, key) pointer stops awaiting the next hold
    extracted_at = {}  # view -> last "Extracted stylesheets" line inside its relayout

    def key_for(view, host):
        host = host or "(none)"
        return f"{host}#v{view}" if by_view and view is not None else host

    def close_visit(view, ts):
        if view in visit_start:
            host, start = visit_start.pop(view)
            sites[key_for(view, host)].dwell_s += max(0.0, (ts - start).total_seconds())

    with open(path, "r", errors="replace") as fh:
        for raw in fh:
            line = ANSI.sub("", raw.rstrip("\n"))
            m = TS.match(line)
            if not m:
                continue
            ts = datetime.fromisoformat(m.group(1)[:26])
            last_ts = ts
            msg = m.group(3)
            vm = VIEW.search(msg)
            view = int(vm.group(1)) if vm else None

            if any(s in msg for s in NAV_SIGNALS):
                leaving.add(view if view is not None else content_view)
            if "Loading URL id=" in msg or "Loading HTML content id=" in msg:
                hm = HOST.search(msg)
                host = hm.group(1) if hm else "(html)"
                loading[view] = host
                content_view = view if content_view is None or "Loading URL" in msg else content_view
                close_visit(view, ts)
                sites[key_for(view, host)].visits += 1
                continue

            cv = view if view is not None else content_view
            if cv in leaving and any(a in msg for a in ALIVE):
                leaving.discard(cv)
                if cv in loading:
                    committed[cv] = loading[cv]
                    if cv not in visit_start:
                        visit_start[cv] = (loading[cv], ts)
            host_live = committed.get(cv) or loading.get(cv)
            host_load = loading.get(cv) or committed.get(cv)
            site = sites[key_for(cv, host_live)]

            if "Extracted stylesheets and CSS variables" in msg:
                extracted_at[view] = ts
            elif "Script stopped at the script budget" in msg:
                d = kv(msg.split("budget", 1)[1])
                src = stop_source(d.get("source", "?"))
                el = num(d, "elapsed_ms")
                kind = "leaving" if (cv in leaving and el == 0) else "live"
                site.stops[src][kind].append(el)
                site.stops[src]["late"].append(num(d, "late_ms"))
                if src.startswith("event:") and src[6:] in POINTER:
                    pending_proxy.append((ts, key_for(cv, host_live)))
            elif "relayout cause=" in msg:
                d = kv(msg.split("relayout cause=", 1)[1])
                cause = msg.split("relayout cause=", 1)[1].split()[0].split("(")[0]
                r = site.relayouts[cause]
                r["took"].append(num(d, "took_ms"))
                for s in STAGES:
                    r[s].append(num(d, s))
                took = num(d, "took_ms")
                started = ts.timestamp() - took / 1000.0
                ext = extracted_at.pop(view, None)
                if "extract_ms" in d:
                    # LAT1: the line names it.
                    r["extract"].append(num(d, "extract_ms"))
                elif ext is not None and started <= ext.timestamp() <= ts.timestamp():
                    # Time from the relayout's start to the end of sheet extraction:
                    # stylesheet + custom-property collection, outside build_ms.
                    r["extract"].append(round((ext.timestamp() - started) * 1000.0, 1))
                r["unstaged"].append(max(0.0, num(d, "took_ms") - sum(num(d, s) for s in STAGES) - num(d, "render_ms")))
                reason = d.get("restyle_miss") or d.get("full_reason") or "-"
                if "restyle_miss=" in msg:
                    reason = "restyle_miss: " + msg.split('restyle_miss="', 1)[1].split('"', 1)[0]
                elif d.get("layout_skipped") == "true":
                    reason = "layout_skipped"
                else:
                    reason = f"{d.get('would_path', '?')}/{d.get('full_reason', '-')}"
                r["reasons"][reason] += 1
                if cause == "initial_load" and cv in loading:
                    committed[cv] = loading[cv]
            elif "Forced layouts in the script task" in msg:
                d = kv(msg)
                site.forced_layouts += int(num(d, "forced_layouts"))
                site.forced_ms += num(d, "ms")
                site.forced_tasks += 1
            elif HOLD_SUMMARY.search(msg):
                hm = HOLD_SUMMARY.search(msg)
                thread, tag_view, tag_host = hm.group(1), hm.group(2), hm.group(3)
                # LAT1b: the window thread's entries are no content visit.
                target = site if thread == "Content" else None
                if target is not None and tag_host is not None:
                    # LAT1b: the line names its visit; the log position may be another's.
                    target = sites[key_for(int(tag_view) if tag_view.isdigit() else None,
                                           "(html)" if tag_host == "-" else tag_host)]
                fields = kv(msg) if target is not None else {}
                if "over_50_ms" in fields:
                    over = fields["over_50_ms"]
                    target.hold_summary_over.extend(float(x) for x in over.split(",") if over != "-")
                    target.hold_summary_tagged_n += int(num(fields, "n"))
                for item in fields.get("by_entry", "").split(","):
                    parts = item.split(":")
                    if len(parts) >= 2 and parts[1].isdigit():
                        target.hold_summary_n[thread][parts[0]] += int(parts[1])
            elif "thread hold entry=" in msg:
                d = kv(msg.split("hold ", 1)[1])
                entry = d.get("entry", "?")
                target = sites[key_for(cv, host_load if entry in ("load", "load_html") else host_live)]
                h = target.holds[entry]
                h["ms"].append(num(d, "ms"))
                thread = "Content" if "Content thread hold" in msg else "Window"
                target.hold_logged[thread].append((entry, num(d, "ms")))
                h["script_ms"].append(num(d, "script_ms"))
                h["other_ms"].append(num(d, "other_ms"))
                if cv in leaving and entry not in ("load", "load_html"):
                    # Input or a live turn run by a document the window is leaving.
                    h["leaving_ms"].append(num(d, "ms"))
                for s in STAGES:
                    h[s].append(num(d, s))
                for pts, pkey in pending_proxy:
                    if (ts - pts).total_seconds() <= PROXY_WINDOW_S:
                        sites[pkey].proxy.append(num(d, "ms"))
                        sites[pkey].proxy_entries[entry] += 1
                pending_proxy = []
            elif "Image over the subresource budget" in msg:
                site.image_drops += 1
            elif "Page lifecycle step done" in msg and "timers" in msg:
                site.timer_callbacks += int(num(kv(msg), "callbacks"))
            elif "Page scripts ran" in msg:
                sites[key_for(cv, host_load)].page_scripts_ran.append(num(kv(msg), "elapsed_ms"))
            elif "Navigation finished" in msg:
                sites[key_for(cv, host_load)].nav_finished.append(num(kv(msg), "elapsed_ms"))
            elif INPUT_SUMMARY.search(msg):
                im = INPUT_SUMMARY.search(msg)
                tag_view, tag_host = im.group(1), im.group(2)
                fields = kv(msg)
                vid = int(tag_view) if tag_view.isdigit() else None
                if fields.get("role") == "chrome":
                    target = sites[key_for(vid, CHROME)]
                else:
                    target = sites[key_for(vid, "(html)" if tag_host == "-" else tag_host)]
                target.input_lines += 1
                target.input_n += int(num(fields, "n"))
                over = fields.get("over_100_ms", "-")
                target.input_over.extend(float(x) for x in over.split(",") if over != "-")
                target.input_no_frame += int(num(fields, "no_frame"))
                for item in fields.get("by_kind", "-").split(","):
                    parts = item.split(":")
                    if len(parts) >= 2 and parts[1].isdigit():
                        target.input_by_kind[parts[0]] += int(parts[1])
            elif "Input to present p50_ms" in msg:
                d = kv(msg)
                site.input_to_present.append(
                    (num(d, "p50_ms"), num(d, "p95_ms"), num(d, "max_ms"), int(num(d, "n")))
                )
            elif "a script turn ran past its budget" in msg:
                site.watchdog.append(num(kv(msg), "ran_ms"))
            # Pointer stops with no hold inside the window get no proxy sample.
            pending_proxy = [p for p in pending_proxy if (ts - p[0]).total_seconds() <= PROXY_WINDOW_S]
    if last_ts is not None:
        for v in list(visit_start):
            close_visit(v, last_ts)


def summarize(sites):
    out = {}
    for name, s in sorted(sites.items()):
        stops = {}
        live_total = leaving_total = 0
        for src, d in sorted(s.stops.items()):
            stops[src] = {
                "live": len(d["live"]),
                "leaving": len(d["leaving"]),
                "elapsed_live": dist(d["live"]),
                "late": dist(d["late"]),
            }
            live_total += len(d["live"])
            leaving_total += len(d["leaving"])
        minutes = s.dwell_s / 60.0
        relayouts = {
            c: {"took": dist(r["took"]), "unstaged": dist(r["unstaged"]), "extract": dist(r["extract"]), "reasons": dict(r["reasons"]),
                **{st: dist(r[st]) for st in STAGES}}
            for c, r in sorted(s.relayouts.items())
        }
        holds = {
            e: {"ms": dist(h["ms"]), "script_ms": dist(h["script_ms"]), "other_ms": dist(h["other_ms"]),
                "leaving_ms": dist(h["leaving_ms"]), **{st: dist(h[st]) for st in STAGES}}
            for e, h in sorted(s.holds.items())
        }
        timer_relayouts = len(s.relayouts.get("timer", {"took": []})["took"])
        hold_entries = hold_entry_counts(s)
        out[name] = {
            "visits": s.visits,
            "dwell_s": round(s.dwell_s, 1),
            "stops_live": live_total,
            "stops_leaving": leaving_total,
            "stops_live_per_min": round(live_total / minutes, 2) if minutes > 0 else None,
            "stops_all_per_min": round((live_total + leaving_total) / minutes, 2) if minutes > 0 else None,
            "stops": stops,
            "relayouts": relayouts,
            "forced_layouts": s.forced_layouts,
            "forced_layout_tasks": s.forced_tasks,
            "forced_layout_ms": round(s.forced_ms, 1),
            "hold_entries": hold_entries,
            "holds": holds,
            "hold_all": dist([x for h in s.holds.values() for x in h["ms"]]),
            "image_budget_drops": s.image_drops,
            "timer_relayouts_per_s": round(timer_relayouts / s.dwell_s, 3) if s.dwell_s > 0 else None,
            "timer_lifecycle_callbacks": s.timer_callbacks,
            "page_scripts_ran_ms": dist(s.page_scripts_ran),
            "navigation_finished_ms": dist(s.nav_finished),
            "input_to_present": s.input_to_present,
            "input_to_present_lat1c": {
                "lines": s.input_lines,
                "n": s.input_n,
                "over_100_ms": sorted(s.input_over, reverse=True),
                "by_kind": dict(s.input_by_kind),
                "no_frame": s.input_no_frame,
            },
            "pointer_stop_to_hold_ms": dist(s.proxy),
            "pointer_stop_hold_entries": dict(s.proxy_entries),
            "watchdog_ran_ms": dist(s.watchdog),
        }
        out[name]["exit_metric"] = exit_metric(s, out[name])
    return out


def hold_thread(s):
    """The thread whose summary lines the metric reads: the content thread,
    when it logged any for the site (LAT1b: never the window thread, whose
    only summary was a headless load and render at the drop)."""
    if s.hold_summary_n.get("Content"):
        return "Content"
    return None


def hold_entry_counts(s):
    thread = hold_thread(s)
    by_entry = dict(s.hold_summary_n[thread]) if thread else {}
    return {"thread": thread, "n": sum(by_entry.values()), "by_entry": by_entry}


def merged_p95(n, logged):
    """Nearest-rank p95 over `n` entries of which `logged` (each at or over
    the line's threshold: 50 ms for holds, 100 ms for inputs) are known:
    (exact p95, False) when it is one of them, (None, True) when it falls
    under the threshold, (None, None) with no entries."""
    n = max(n, len(logged))
    if n == 0:
        return None, None
    k = max(1, min(n, math.ceil(0.95 * n)))
    if len(logged) >= n - k + 1:
        return sorted(logged, reverse=True)[n - k], False
    return None, True


def exit_metric(s, row):
    thread = hold_thread(s)
    entries = row["hold_entries"]["n"]
    if thread and s.hold_summary_tagged_n == entries:
        # LAT1b: every line listed its own long entries; no hold line is needed.
        logged = list(s.hold_summary_over)
    else:
        logged = [ms for _, ms in s.hold_logged.get(thread, [])] if thread else []
    p95, under = merged_p95(entries, logged) if thread else (None, None)
    # LAT1c: exact over every headline input when the site has tagged
    # lines; before LAT1c, the worst untagged 100-input window.
    itp_p95, itp_under = (None, None)
    if s.input_lines:
        itp_p95, itp_under = merged_p95(s.input_n, s.input_over)
        itp = itp_p95
    else:
        itp = max((w[1] for w in s.input_to_present), default=None)
    stops = row["stops_live_per_min"]
    stops_ok = (stops if stops is not None else (0.0 if row["stops_live"] == 0 else None))
    hold_ok = None if under is None else (under or p95 <= EXIT_HOLD_P95_MS)
    verdict = None
    if hold_ok is not None and stops_ok is not None:
        verdict = (stops_ok <= EXIT_STOPS_PER_MIN and hold_ok
                   and (itp is None or itp <= EXIT_INPUT_P95_MS))
    if verdict is None and s.input_lines and stops_ok is None and hold_ok is None and itp_under is not None:
        # The chrome (or a site with only input lines): the input half alone.
        verdict = bool(itp_under or itp <= EXIT_INPUT_P95_MS)
    return {
        "stops_live_per_min": stops,
        "hold_thread": thread,
        "hold_n": entries,
        "hold_logged_over_threshold": len(logged),
        "hold_p95_ms": p95,
        "hold_p95_under_threshold": under,
        "input_to_present_p95_worst_ms": itp,
        "input_to_present_n": s.input_n if s.input_lines else None,
        "input_to_present_p95_ms": itp_p95,
        "input_to_present_p95_under_threshold": itp_under,
        "pass": verdict,
    }


def fmt_d(d, keys=("p50", "p95", "max")):
    if not d or not d.get("n"):
        return "-"
    return "/".join(f"{d[k]:g}" for k in keys)


def print_table(summary, out=sys.stdout):
    w = out.write
    w("site | visits | dwell s | stops live/leaving | live stops/min | relayouts (n, sum ms) | "
      "forced layouts | holds n, p50/p95/max ms | img drops | timer relayouts/s | "
      "scripts ran ms p50/p95/max | nav finished ms p50/p95/max | ptr-stop->hold ms p50/p95/max (n)\n")
    w("|".join(["---"] * 13) + "\n")
    for name, s in summary.items():
        rel_n = sum(r["took"]["n"] for r in s["relayouts"].values())
        rel_ms = sum(r["took"].get("sum", 0) for r in s["relayouts"].values())
        w(" | ".join(str(x) for x in [
            name, s["visits"], s["dwell_s"], f"{s['stops_live']}/{s['stops_leaving']}",
            s["stops_live_per_min"] if s["stops_live_per_min"] is not None else "-",
            f"{rel_n}, {rel_ms:.0f}", s["forced_layouts"],
            f"{s['hold_all']['n']}, {fmt_d(s['hold_all'])}", s["image_budget_drops"],
            s["timer_relayouts_per_s"] if s["timer_relayouts_per_s"] is not None else "-",
            fmt_d(s["page_scripts_ran_ms"]), fmt_d(s["navigation_finished_ms"]),
            f"{fmt_d(s['pointer_stop_to_hold_ms'])} ({s['pointer_stop_to_hold_ms']['n']})",
        ]) + "\n")
    w("\n")
    for name, s in summary.items():
        if not (s["stops"] or s["relayouts"] or s["holds"] or s["input_to_present_lat1c"]["lines"]):
            continue
        w(f"## {name}\n")
        if s["stops"]:
            w("stops by source: " + ", ".join(
                f"{src} {d['live']}+{d['leaving']}lv (live elapsed {fmt_d(d['elapsed_live'])}, late {fmt_d(d['late'])})"
                for src, d in s["stops"].items()) + "\n")
        if s["relayouts"]:
            w("relayouts by cause (n, took p50/p95/max, sum; build p50/p95; layout p50/p95; unstaged p50/p95; sheet-extract p50/p95; path reasons): " + "; ".join(
                f"{c} {r['took']['n']}, {fmt_d(r['took'])}, {r['took'].get('sum', 0):.0f}; "
                f"b {fmt_d(r['build_ms'], ('p50', 'p95'))}; l {fmt_d(r['layout_ms'], ('p50', 'p95'))}; "
                f"unstaged {fmt_d(r['unstaged'], ('p50', 'p95'))}; sheet-extract {fmt_d(r['extract'], ('p50', 'p95'))}; "
                f"{r['reasons']}"
                for c, r in s["relayouts"].items()) + "\n")
        if s["holds"]:
            w("holds by entry (n, ms p50/p95/max, sum; build/layout/display_list/snapshot p50/p95): " + "; ".join(
                f"{e} {h['ms']['n']}, {fmt_d(h['ms'])}, {h['ms'].get('sum', 0):.0f}; "
                + " ".join(f"{st[:-3]} {fmt_d(h[st], ('p50', 'p95'))}" for st in STAGES)
                for e, h in s["holds"].items()) + "\n")
        lat = s["input_to_present_lat1c"]
        if lat["lines"]:
            m = s["exit_metric"]
            p95 = (f"{m['input_to_present_p95_ms']:g}" if m["input_to_present_p95_ms"] is not None
                   else f"< {EXIT_INPUT_P95_MS:g}" if m["input_to_present_p95_under_threshold"] else "-")
            w(f"input to present (LAT1c): n={lat['n']} p95 {p95} ms, over 100 ms {len(lat['over_100_ms'])}, "
              f"by kind {lat['by_kind']}, no frame {lat['no_frame']}\n")
        if s["input_to_present"]:
            w("input to present (p50/p95/max n): " + ", ".join(
                f"{a:g}/{b:g}/{c:g} n={n}" for a, b, c, n in s["input_to_present"]) + "\n")
        if s["pointer_stop_hold_entries"]:
            w(f"pointer-stop -> hold entries: {s['pointer_stop_hold_entries']}\n")
        if s["watchdog_ran_ms"]["n"]:
            w(f"watchdog interrupts ran_ms: {fmt_d(s['watchdog_ran_ms'])} n={s['watchdog_ran_ms']['n']}\n")
        w(f"forced layouts {s['forced_layouts']} in {s['forced_layout_tasks']} tasks, "
          f"{s['forced_layout_ms']:g} ms; "
          f"timer lifecycle callbacks {s['timer_lifecycle_callbacks']}\n")
        if s["hold_entries"]["n"]:
            w(f"hold entries ({s['hold_entries']['thread']} thread, all lengths): n={s['hold_entries']['n']} "
              f"by entry {s['hold_entries']['by_entry']}\n")
        w("\n")
    print_exit_metric(summary, out)


def print_exit_metric(summary, out=sys.stdout):
    w = out.write
    w(f"## exit metric (live stops/min <= {EXIT_STOPS_PER_MIN:g}, hold p95 over all entries <= "
      f"{EXIT_HOLD_P95_MS:g} ms, input-to-present p95 <= {EXIT_INPUT_P95_MS:g} ms: over every input "
      f"(LAT1c), else the worst window)\n")
    w("site | live stops/min | hold entries n (logged >= 50) | hold p95 ms | input-to-present p95 ms (n) | pass\n")
    w("|".join(["---"] * 6) + "\n")
    for name, s in summary.items():
        m = s["exit_metric"]
        if not (s["visits"] or m["hold_n"] or s["stops_live"] or s["input_to_present"]
                or s["input_to_present_lat1c"]["lines"]):
            continue
        if m["hold_p95_ms"] is not None:
            p95 = f"{m['hold_p95_ms']:g}"
        elif m["hold_p95_under_threshold"]:
            p95 = f"< {HOLD_THRESHOLD_MS:g}"
        else:
            p95 = "n/a (no content-thread entries)"
        verdict = {True: "yes", False: "NO", None: "n/a"}[m["pass"]]
        w(" | ".join(str(x) for x in [
            name, "-" if m["stops_live_per_min"] is None else m["stops_live_per_min"],
            f"{m['hold_n']} ({m['hold_logged_over_threshold']})", p95,
            input_cell(m),
            verdict,
        ]) + "\n")


def input_cell(m):
    if m["input_to_present_n"] is not None:
        if m["input_to_present_p95_ms"] is not None:
            p95 = f"{m['input_to_present_p95_ms']:g}"
        elif m["input_to_present_p95_under_threshold"]:
            p95 = f"< {EXIT_INPUT_P95_MS:g}"
        else:
            p95 = "-"
        return f"{p95} ({m['input_to_present_n']})"
    if m["input_to_present_p95_worst_ms"] is None:
        return "-"
    return f"{m['input_to_present_p95_worst_ms']:g} (worst window)"


def rank(summary):
    """Totals across sites. Holds never nest (one per outermost entry), and
    relayouts never nest, so each family sums without double counting; the
    two families overlap (a hover relayout runs inside a move hold)."""
    holds = defaultdict(lambda: defaultdict(float))
    relayouts = defaultdict(lambda: defaultdict(float))
    cross = defaultdict(float)
    for s in summary.values():
        for e, h in s["holds"].items():
            t = holds[e]
            t["n"] += h["ms"]["n"]
            for k in ("ms", "script_ms", "other_ms", "leaving_ms") + STAGES:
                t[k] += h[k].get("sum", 0.0)
        for c, r in s["relayouts"].items():
            t = relayouts[c]
            t["n"] += r["took"]["n"]
            t["took"] += r["took"].get("sum", 0.0)
            t["extract"] += r["extract"].get("sum", 0.0)
            for k in STAGES:
                t[k] += r[k].get("sum", 0.0)
            cross["sheet_extract_ms"] += r["extract"].get("sum", 0.0)
        cross["forced_layouts"] += s["forced_layouts"]
        cross["forced_layout_ms"] += s["forced_layout_ms"]
        cross["hold_entries"] += s["hold_entries"]["n"]
        cross["image_budget_drops"] += s["image_budget_drops"]
        cross["watchdog_ms"] += s["watchdog_ran_ms"].get("sum", 0.0)
        cross["stops_live"] += s["stops_live"]
        cross["stops_leaving"] += s["stops_leaving"]
        cross["dwell_s"] += s["dwell_s"]
    return {
        "holds": dict(sorted(((k, dict(v)) for k, v in holds.items()), key=lambda kv: -kv[1]["ms"])),
        "relayouts": dict(sorted(((k, dict(v)) for k, v in relayouts.items()), key=lambda kv: -kv[1]["took"])),
        "cross": dict(cross),
    }


def print_rank(r, out=sys.stdout):
    w = out.write
    w("holds by entry (total ms; n; script / build / layout / display_list / snapshot / other; while leaving)\n")
    for e, t in r["holds"].items():
        w(f"  {e:10} {t['ms']:9.0f} n={t['n']:4.0f}  script {t['script_ms']:.0f} / build {t['build_ms']:.0f} / "
          f"layout {t['layout_ms']:.0f} / dl {t['display_list_ms']:.0f} / snap {t['snapshot_ms']:.0f} / "
          f"other {t['other_ms']:.0f}; leaving {t['leaving_ms']:.0f}\n")
    w("relayouts by cause (total took ms; n; sheet-extract / build / layout / display_list / snapshot)\n")
    for c, t in r["relayouts"].items():
        w(f"  {c:16} {t['took']:9.0f} n={t['n']:4.0f}  extract {t['extract']:.0f} / build {t['build_ms']:.0f} / "
          f"layout {t['layout_ms']:.0f} / dl {t['display_list_ms']:.0f} / snap {t['snapshot_ms']:.0f}\n")
    w("cross-cutting: " + ", ".join(f"{k} {v:.0f}" for k, v in r["cross"].items()) + "\n")


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("logs", nargs="+")
    ap.add_argument("--json", action="store_true", help="print the summary as JSON")
    ap.add_argument("--rank", action="store_true", help="print totals by mechanism across all sites")
    ap.add_argument("--views", action="store_true", help="split sites by engine view id")
    args = ap.parse_args(argv)
    summary = summarize(parse(args.logs, by_view=args.views))
    if args.rank:
        print_rank(rank(summary))
    elif args.json:
        json.dump(summary, sys.stdout, indent=1)
        sys.stdout.write("\n")
    else:
        print_table(summary)


if __name__ == "__main__":
    main()
