"""Binaries differ? And the board's site ids. usage: pre.py <binA> <binB>"""
import hashlib, json, sys
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
for b in sys.argv[1:3]:
    print(hashlib.sha256(open(b, 'rb').read()).hexdigest()[:16], b)
j = json.load(open(f'{HUB}/websuite/realsite-top20.json'))
s = j['sites'] if isinstance(j, dict) else j
print(' '.join(x['id'] for x in s))
