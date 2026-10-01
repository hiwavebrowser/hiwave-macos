#!/usr/bin/env python3
"""Combine several ab_flag.py logs into one set of per-pair medians.

    ab_combine.py <log> [<log> ...]
"""
import ast
import re
import statistics as st
import sys

S = ("cnn", "github", "wikipedia")
CHROME = dict(cnn=210, github=110, wikipedia=20)
PAT = re.compile(
    r"== pair (\d+) order (\w\w) (\w) .*load=([\d.]+) cnn=(\d+)/\db github=(\d+)/\db "
    r"wikipedia=(\d+)/\db \| (\S+) (\S+) (\S+)"
)
rows = []
for f in sys.argv[1:]:
    cur = {}
    for line in open(f):
        m = PAT.match(line)
        if not m:
            continue
        r = cur.setdefault(m[1], dict(order=m[2]))
        r[m[3]] = dict(
            load=float(m[4]),
            tot=dict(zip(S, map(int, m.group(5, 6, 7)))),
            pb=dict(zip(S, map(ast.literal_eval, m.group(8, 9, 10)))),
        )
    rows += cur.values()
print(len(rows), "pairs; AB", sum(r["order"] == "AB" for r in rows),
      "BA", sum(r["order"] == "BA" for r in rows))
loads = [r[a]["load"] for r in rows for a in "AB"]
print("load range", min(loads), max(loads))
for s in S:
    ra = [r["B"]["tot"][s] / r["A"]["tot"][s] for r in rows]
    A = [r["A"]["tot"][s] for r in rows]
    B = [r["B"]["tot"][s] for r in rows]
    fa = [r["A"]["pb"][s][0] for r in rows]
    fb = [r["B"]["pb"][s][0] for r in rows]
    sa = [r["A"]["pb"][s][1] for r in rows]
    sb = [r["B"]["pb"][s][1] for r in rows]
    fr = [r["B"]["pb"][s][0] / r["A"]["pb"][s][0] for r in rows]
    print(s, "median B/A %.3f" % st.median(ra), "below1", sum(x < 1 for x in ra),
          "range %.2f-%.2f" % (min(ra), max(ra)))
    print("   A med", st.median(A), "-> %.1fx" % (st.median(A) / CHROME[s]),
          "| B med", st.median(B), "-> %.1fx" % (st.median(B) / CHROME[s]))
    print("   first build med A %.1f B %.1f | min A %.1f B %.1f | max A %.1f B %.1f"
          % (st.median(fa), st.median(fb), min(fa), min(fb), max(fa), max(fb)))
    print("   second build med A %.1f B %.1f | first-build B/A per-pair median %.3f, below 1 in %d"
          % (st.median(sa), st.median(sb), st.median(fr), sum(x < 1 for x in fr)))
