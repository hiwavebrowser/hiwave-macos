"""Append this session's digest and PLAN note, commit the hub artifacts, push atlas/trench-realsite."""
import glob, subprocess
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
S = f'{HUB}/scratch/s1001c'
for target, part in (('trench/digest-realsite.md', 'digest-0750.md'), ('trench/PLAN-realsite.md', 'plan-0750.md')):
    body = open(f'{S}/{part}').read()
    cur = open(f'{HUB}/{target}').read()
    if body.strip() not in cur:
        open(f'{HUB}/{target}', 'a').write(('' if cur.endswith('\n') else '\n') + body)
paths = ['trench/digest-realsite.md', 'trench/PLAN-realsite.md']
for pat in ('scratch/s1001c/*.py', 'scratch/s1001c/*.md', 'scratch/s1001c/*.txt',
            'scratch/s1001c/rc-ab-rows34.png', 'scratch/s1001c/ab-facebook.png', 'scratch/s1001c/ab-shopify.png',
            'scratch/s0929e/camp-dev-f16ad4e/*.json', 'scratch/s0929e/camp-dev-f16ad4e/ratchet.txt',
            'scratch/s0929e/camp-ellipse-740efcb/*.json', 'scratch/s0929e/camp-ellipse-740efcb/ratchet.txt'):
    paths += [p[len(HUB) + 1:] for p in glob.glob(f'{HUB}/{pat}')]
for cmd in (['git', 'add', '--'] + paths,
            ['git', 'commit', '-q', '-F', 'scratch/s1001c/commit-digest.txt'],
            ['git', 'push', '-q', 'origin', 'atlas/trench-realsite'],
            ['git', 'log', '--oneline', '-1'],
            ['git', 'status', '-sb', '--untracked-files=no']):
    r = subprocess.run(cmd, cwd=HUB, capture_output=True, text=True)
    print(cmd[1], r.returncode, (r.stdout + r.stderr).strip()[-300:])
