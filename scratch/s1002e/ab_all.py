"""All-site frames A/B, develop arm against the background-fetch arm; ab-bgf.txt, DONE at the end.
Then the same frames against the stored Chrome frames (vs-bgf.txt)."""
import json, subprocess
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
D = f'{HUB}/scratch/s1002e'
dev, fix = f'{HUB}/scratch/bin/pc-dev-f0d5fa2', f'{HUB}/scratch/bin/pc-bgf-wip'
j = json.load(open(f'{HUB}/websuite/realsite-top20.json'))
sites = [s['id'] for s in (j['sites'] if isinstance(j, dict) else j)]
with open(f'{D}/ab-bgf.txt', 'w') as out:
    subprocess.run(['python3', f'{HUB}/scratch/s1001g/ab.py', dev, fix] + sites, stdout=out, stderr=subprocess.STDOUT)
    out.write('DONE\n')
with open(f'{D}/vs-bgf.txt', 'w') as out:
    subprocess.run(['python3', f'{HUB}/scratch/s1001g/vs_chrome.py'] + sites, stdout=out, stderr=subprocess.STDOUT)
    out.write('DONE\n')
