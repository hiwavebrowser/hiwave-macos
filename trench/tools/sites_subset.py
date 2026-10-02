#!/usr/bin/env python3
"""sites_subset.py <sites.json> <out.json> <id> [...]: the board site list cut down to the named ids."""
import json
import sys

cfg = json.load(open(sys.argv[1]))
cfg["sites"] = [s for s in cfg["sites"] if s["id"] in sys.argv[3:]]
json.dump(cfg, open(sys.argv[2], "w"))
print([s["id"] for s in cfg["sites"]])
