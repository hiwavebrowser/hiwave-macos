#!/usr/bin/env python3
"""Tables from a chrome_groundtruth.mjs run dir: phases, libraries, coverage, APIs.

  python3 trench/tools/analysis/groundtruth_report.py <run_dir>   # markdown to stdout
"""
import json, sys, collections, statistics
from pathlib import Path

run = Path(sys.argv[1])
# Pages that run no script at all (ebay/nytimes block pages) still show 10 Proxy
# constructions: the harness/Playwright baseline, subtracted below.
PROXY_BASELINE = 10
R = json.loads((run / "summary.json").read_text())["results"]

def blocked(r):
    if r.get("status") in ("blocked", "timeout", "probe-error", "missing"): return True
    c = r.get("clean", {})
    t = (c.get("title") or r.get("census", {}).get("title") or "").lower()
    n = r.get("census", {}).get("nodes") or 0
    return r["status"] != "ok" or n < 60 or any(w in t for w in ("denied", "just a moment", "access", "blocked", "robot", "attention"))

ok = [r for r in R if not blocked(r)]
print(f"sites: {len(R)}, scorable (not blocked/challenge): {len(ok)}; blocked: {', '.join(r['id'] for r in R if blocked(r))}\n")

print("### Phases (Chrome main thread, self ms, trace-only run) and clean load/FCP\n")
print("| site | clean FCP | clean load | parse | script | style (recalcs/elements) | layout (count/dirty) | paint | gc (minor/major) | other | JS heap MB | DOM |")
print("|---|---|---|---|---|---|---|---|---|---|---|---|")
for r in R:
    t = r.get("trace") or {}; s = t.get("self_ms", {}); c = r.get("clean", {}); h = r.get("heap", {})
    mark = " (blocked)" if blocked(r) else ""
    print(f"| {r['id']}{mark} | {c.get('fcp_ms')} | {c.get('load_ms')} | {s.get('parse')} | {s.get('script')} | "
          f"{s.get('style')} ({t.get('style_recalcs')}/{t.get('style_elements')}) | {s.get('layout')} ({t.get('layout')}/{t.get('layout_dirty_objects')}) | "
          f"{s.get('paint')} | {s.get('gc')} ({t.get('minor_gc')}/{t.get('major_gc')}) | {s.get('other')} | {h.get('js_heap_used_mb')} | {h.get('dom_nodes')} |")

def med(k1, k2):
    v = [r["trace"]["self_ms"][k2] for r in ok if r.get("trace") and r["trace"].get("self_ms")]
    return round(statistics.median(v), 1) if v else None
print("\nMedian over scorable sites (self ms): " + ", ".join(f"{k} {med('self_ms', k)}" for k in ("parse", "script", "style", "layout", "paint", "gc", "other")))
tot = collections.Counter()
for r in ok:
    for k, v in (r.get("trace") or {}).get("self_ms", {}).items(): tot[k] += v
attrib = sum(v for k, v in tot.items() if k != "other")
print("Share of attributed main-thread time across scorable sites: " + ", ".join(f"{k} {100*tot[k]/attrib:.0f}%" for k in ("script", "style", "layout", "parse", "paint", "gc")))

print("\n### Coverage\n")
print("| site | scripts | JS KB | JS run @FCP | JS run @load+settle | CSS KB | CSS used @FCP | CSS used @end |")
print("|---|---|---|---|---|---|---|---|")
for r in R:
    j, c = r.get("js", {}), r.get("css", {})
    print(f"| {r['id']}{' (blocked)' if blocked(r) else ''} | {j.get('scripts')} | {round((j.get('bytes_total') or 0)/1024)} | {j.get('pct_exec_at_fcp')}% | {j.get('pct_exec_at_end')}% | "
          f"{round((c.get('bytes_total') or 0)/1024)} | {c.get('pct_bytes_used_at_fcp')}% | {c.get('pct_bytes_used_at_end')}% |")
jt = sum(r["js"]["bytes_total"] for r in ok); je = sum(r["js"]["bytes_exec_at_end"] for r in ok)
jf = sum((r["js"]["bytes_exec_at_fcp"] or 0) for r in ok)
ct = sum(r["css"]["bytes_total"] for r in ok)
print(f"\nPooled over scorable sites: JS {jt/1048576:.1f} MB shipped, {100*jf/jt:.0f}% executed by FCP, {100*je/jt:.0f}% by load+settle; CSS {ct/1048576:.1f} MB.")
pe = sorted(r["js"]["pct_exec_at_end"] for r in ok if r["js"]["pct_exec_at_end"] is not None)
print(f"Per-site JS executed by load+settle: median {statistics.median(pe):.0f}%, range {pe[0]:.0f}–{pe[-1]:.0f}%.")

print("\n### Library census (scorable sites)\n")
keys = ["react", "next", "vue", "nuxt", "angular", "svelte", "lit", "wc_polyfill", "core_js", "gtm", "gtag", "ga", "segment", "optimizely", "adobe_launch", "ads"]
cnt = {k: [r["id"] for r in ok if r.get("census", {}).get(k)] for k in keys}
jq = [f"{r['id']} {r['census']['jquery']}" for r in ok if r.get("census", {}).get("jquery")]
print("| library | sites | which |\n|---|---|---|")
for k in sorted(keys, key=lambda k: -len(cnt[k])):
    print(f"| {k} | {len(cnt[k])} | {', '.join(cnt[k])} |")
print(f"| jquery | {len(jq)} | {', '.join(jq)} |")
ce = [(r["id"], r["census"].get("custom_elements_defined", 0), r["census"].get("custom_element_tags", 0), r["census"].get("shadow_roots_open", 0)) for r in ok]
print("\nCustom elements (defined/tags) and open shadow roots: " + "; ".join(f"{i} {d}/{t}, {s} shadow" for i, d, t, s in ce if t or s))

print("\n### Web APIs called during load (scorable sites)\n")
api_sites = collections.Counter(); api_calls = collections.Counter()
for r in ok:
    for k, v in (r.get("census", {}).get("api") or {}).items():
        if k == "Proxy": v = max(0, v - PROXY_BASELINE)
        if v: api_sites[k] += 1; api_calls[k] += v
print("| API | sites | total calls | median calls/site (where used) |\n|---|---|---|---|")
for k, n in api_sites.most_common():
    per = [r["census"]["api"].get(k, 0) - (PROXY_BASELINE if k == "Proxy" else 0) for r in ok if (r["census"].get("api") or {}).get(k)]
    per = [p for p in per if p > 0]
    print(f"| {k} | {n} | {api_calls[k]} | {int(statistics.median(per))} |")

print("\n### WeakMap / WeakRef / FinalizationRegistry during load\n")
print("| site | WeakMap | WeakSet | WeakRef | FinalizationRegistry | Proxy | Promise |\n|---|---|---|---|---|---|---|")
for r in ok:
    a = r.get("census", {}).get("api") or {}
    print(f"| {r['id']} | {a.get('WeakMap',0)} | {a.get('WeakSet',0)} | {a.get('WeakRef',0)} | {a.get('FinalizationRegistry',0)} | {max(0, a.get('Proxy',0) - PROXY_BASELINE)} | {a.get('Promise',0)} |")
