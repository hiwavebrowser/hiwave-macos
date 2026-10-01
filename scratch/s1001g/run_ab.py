"""run_ab.py from s1001f, but ab.py runs under the interpreter running this script (the PATH python3
lost PIL on 2026-10-01 evening; anaconda's has it).
usage: <python-with-PIL> run_ab.py <binA> <binB> <site>..."""
import json, subprocess, sys

HUB = "/Users/petecopeland/Repos/.worktrees/trench-realsite"
j = json.load(open(f"{HUB}/websuite/realsite-top20.json"))
sites = j["sites"] if isinstance(j, dict) else j
urls = {s["id"]: s["url"] for s in sites}
specs = [f"{n}={urls[n]}" for n in sys.argv[3:]]
subprocess.run([sys.executable, f"{HUB}/scratch/s1001f/ab.py", sys.argv[1], sys.argv[2], *specs])
