"""LOADS detail of sites in two board runs.
usage: loads.py <runA> <runB> <site>..."""
import json, os, sys

for r in sys.argv[1:3]:
    for s in sys.argv[3:]:
        z = json.load(open(os.path.join("trench/realsite/runs", r, s + ".json")))
        rk = z["rustkit"]
        print(r[-12:], s, json.dumps(z["loads"])[:400])
        short = {}
        for k, v in rk.items():
            short[k] = v if not isinstance(v, (list, dict)) else type(v).__name__ + str(len(v))
        print("   rk", short)
