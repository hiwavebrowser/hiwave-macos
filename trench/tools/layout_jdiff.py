#!/usr/bin/env python3
"""layout_jdiff.py <a.json> <b.json> [max]: list the paths at which two JSON dumps differ."""
import json
import re
import sys

a, b = (json.load(open(p)) for p in sys.argv[1:3])
limit = int(sys.argv[3]) if len(sys.argv) > 3 else 40
out = []


def norm(v):
    return re.sub(r"127\.0\.0\.1:\d+", "HOST", v) if isinstance(v, str) else v


def walk(x, y, path):
    if len(out) >= 5000:
        return
    if isinstance(x, dict) and isinstance(y, dict):
        for k in sorted(set(x) | set(y)):
            if k not in x or k not in y:
                out.append((path + "/" + k, "only in " + ("a" if k in x else "b"), ""))
            else:
                walk(x[k], y[k], path + "/" + k)
    elif isinstance(x, list) and isinstance(y, list):
        if len(x) != len(y):
            out.append((path, "len %d" % len(x), "len %d" % len(y)))
        for i, (p, q) in enumerate(zip(x, y)):
            walk(p, q, "%s[%d]" % (path, i))
    elif norm(x) != norm(y):
        out.append((path, repr(x)[:80], repr(y)[:80]))


walk(a, b, "")
print(len(out), "differences")
keys = {}
for p, _, _ in out:
    k = p.rsplit("/", 1)[-1]
    keys[k] = keys.get(k, 0) + 1
print(sorted(keys.items(), key=lambda kv: -kv[1])[:15])
for row in out[:limit]:
    print(*row)
