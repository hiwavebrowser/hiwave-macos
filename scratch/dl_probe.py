"""Count transform commands and find a text run in two display lists.
usage: dl_probe.py <needle> <display-list.json> [...]"""
import json, sys

needle = sys.argv[1]
for path in sys.argv[2:]:
    d = json.load(open(path))
    cmds = d["commands"] if isinstance(d, dict) else d
    kinds = {}
    for c in cmds:
        k = next(iter(c)) if isinstance(c, dict) else str(c)[:20]
        if isinstance(c, dict) and "type" in c:
            k = c["type"]
        kinds[k] = kinds.get(k, 0) + 1
    hits = [c for c in cmds if needle in json.dumps(c)]
    tf = {k: v for k, v in kinds.items() if "ransform" in k.lower()}
    print(path, "transform cmds:", tf, "total", len(cmds))
    print("  hits:", json.dumps(hits)[:500])
