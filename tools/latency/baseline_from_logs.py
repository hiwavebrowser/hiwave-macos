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
* ``Forced layouts in the script task`` (count of forced layouts);
* ``Content thread hold`` / ``Window thread hold`` lines by entry, with
  ``ms`` and the build/layout/display_list/snapshot stages (only holds of
  50 ms or more are logged: ``wgate::HOLD_THRESHOLD_MS``);
* ``Image over the subresource budget`` drops;
* timer pressure: ``relayout cause=timer`` per second of dwell, and the
  ``Page lifecycle step done source="timers"`` callbacks;
* ``Page scripts ran`` and ``Navigation finished`` elapsed_ms;
* ``Input to present`` summary lines;
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
        self.holds = defaultdict(lambda: {
            "ms": [], **{s: [] for s in STAGES}, "script_ms": [], "other_ms": [], "leaving_ms": []})
        self.image_drops = 0
        self.timer_callbacks = 0
        self.page_scripts_ran = []
        self.nav_finished = []
        self.input_to_present = []  # (p50, p95, max, n)
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
                if ext is not None and started <= ext.timestamp() <= ts.timestamp():
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
                site.forced_layouts += int(num(kv(msg), "forced_layouts"))
                site.forced_tasks += 1
            elif "thread hold entry=" in msg:
                d = kv(msg.split("hold ", 1)[1])
                entry = d.get("entry", "?")
                target = sites[key_for(cv, host_load if entry in ("load", "load_html") else host_live)]
                h = target.holds[entry]
                h["ms"].append(num(d, "ms"))
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
            "holds": holds,
            "hold_all": dist([x for h in s.holds.values() for x in h["ms"]]),
            "image_budget_drops": s.image_drops,
            "timer_relayouts_per_s": round(timer_relayouts / s.dwell_s, 3) if s.dwell_s > 0 else None,
            "timer_lifecycle_callbacks": s.timer_callbacks,
            "page_scripts_ran_ms": dist(s.page_scripts_ran),
            "navigation_finished_ms": dist(s.nav_finished),
            "input_to_present": s.input_to_present,
            "pointer_stop_to_hold_ms": dist(s.proxy),
            "pointer_stop_hold_entries": dict(s.proxy_entries),
            "watchdog_ran_ms": dist(s.watchdog),
        }
    return out


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
        if not (s["stops"] or s["relayouts"] or s["holds"]):
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
        if s["input_to_present"]:
            w("input to present (p50/p95/max n): " + ", ".join(
                f"{a:g}/{b:g}/{c:g} n={n}" for a, b, c, n in s["input_to_present"]) + "\n")
        if s["pointer_stop_hold_entries"]:
            w(f"pointer-stop -> hold entries: {s['pointer_stop_hold_entries']}\n")
        if s["watchdog_ran_ms"]["n"]:
            w(f"watchdog interrupts ran_ms: {fmt_d(s['watchdog_ran_ms'])} n={s['watchdog_ran_ms']['n']}\n")
        w(f"forced layouts {s['forced_layouts']} in {s['forced_layout_tasks']} tasks; "
          f"timer lifecycle callbacks {s['timer_lifecycle_callbacks']}\n\n")


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
