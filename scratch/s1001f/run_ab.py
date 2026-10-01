"""Run ab.py on named board sites (URLs from the pinned list).
usage: run_ab.py <binA> <binB> <site>..."""
import json, subprocess, sys

HUB = "/Users/petecopeland/Repos/.worktrees/trench-realsite"
j = json.load(open(f"{HUB}/websuite/realsite-top20.json"))
sites = j["sites"] if isinstance(j, dict) else j
urls = {s["id"]: s["url"] for s in sites}
specs = [f"{n}={urls[n]}" for n in sys.argv[3:]]
subprocess.run(["python3", f"{HUB}/scratch/s1001f/ab.py", sys.argv[1], sys.argv[2], *specs])
