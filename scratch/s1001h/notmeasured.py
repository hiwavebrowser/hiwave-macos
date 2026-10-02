"""Why a campaign arm has NOT-MEASURED cases: the result rows and the log lines around them.
usage: notmeasured.py <arm-label> [scope]"""
import json, sys
D = f'/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/s0929e/camp-{sys.argv[1]}'
scope = sys.argv[2] if len(sys.argv) > 2 else 'all'
j = json.load(open(f'{D}/{scope}.json'))
for r in j['results']:
    if not isinstance(r.get('diff_pct'), (int, float)) or r.get('error') or r.get('status') not in (None, 'ok', 'pass', 'passed', 'fail', 'failed'):
        print({k: (str(v)[:300]) for k, v in r.items() if k not in ('attribution', 'taxonomy')})
lines = open(f'{D}/{scope}.log').read().splitlines()
for i, l in enumerate(lines):
    low = l.lower()
    if 'not-measured' in low or 'timeout' in low or 'timed out' in low or 'instrument' in low:
        print(i, l[:300])
