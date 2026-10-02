"""The image-routing arm's campaign, then the all-site A/B against the develop arm, one after the other.
Writes camp-img.txt and ab-img.txt beside this file; DONE at the end of ab-img.txt."""
import json, subprocess
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
D = f'{HUB}/scratch/s1002d'
dev, fix = f'{HUB}/scratch/bin/pc-dev-60f7d39', f'{HUB}/scratch/bin/pc-img-ef58355'
r = subprocess.run(['python3', f'{HUB}/scratch/s1001c/camp3.py', 'img-ef58355', fix], capture_output=True, text=True)
open(f'{D}/camp-img.txt', 'w').write(r.stdout[-4000:] + r.stderr[-1000:])
j = json.load(open(f'{HUB}/websuite/realsite-top20.json'))
sites = [s['id'] for s in (j['sites'] if isinstance(j, dict) else j)]
with open(f'{D}/ab-img.txt', 'w') as out:
    subprocess.run(['python3', f'{HUB}/scratch/s1001g/ab.py', dev, fix] + sites, stdout=out, stderr=subprocess.STDOUT)
    out.write('DONE\n')
