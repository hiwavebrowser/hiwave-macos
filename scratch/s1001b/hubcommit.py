"""Append this session's digest and PLAN note, commit the hub artifacts, push atlas/trench-realsite."""
import glob, subprocess
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
S = f'{HUB}/scratch/s1001b'
for target, part in (('trench/digest-realsite.md', 'digest-0400.md'), ('trench/PLAN-realsite.md', 'plan-0400.md')):
    body = open(f'{S}/{part}').read()
    cur = open(f'{HUB}/{target}').read()
    if body.strip() not in cur:
        open(f'{HUB}/{target}', 'a').write(('' if cur.endswith('\n') else '\n') + body)
paths = ['trench/digest-realsite.md', 'trench/PLAN-realsite.md']
for pat in ('scratch/s1001b/*.py', 'scratch/s1001b/*.md', 'scratch/s1001b/*.txt',
            'scratch/s1001b/woff-dev-e4a82f7-zoom.png', 'scratch/s1001b/woff-fix-zoom.png',
            'scratch/s1001b/shopify-ab.png', 'scratch/s1001b/google-ab.png',
            'scratch/s0929e/camp-dev-e4a82f7/*.json', 'scratch/s0929e/camp-dev-e4a82f7/ratchet.txt',
            'scratch/s0929e/camp-woff-fe23762/*.json', 'scratch/s0929e/camp-woff-fe23762/ratchet.txt',
            'trench/realsite/runs/20261001T0715Z-woff-ab-*/*.json'):
    paths += [p[len(HUB) + 1:] for p in glob.glob(f'{HUB}/{pat}')]
for cmd in (['git', 'add', '--'] + paths,
            ['git', 'commit', '-q', '-F', 'scratch/s1001b/commit-digest.txt'],
            ['git', 'push', '-q', 'origin', 'atlas/trench-realsite'],
            ['git', 'log', '--oneline', '-1'],
            ['git', 'status', '-sb', '--untracked-files=no']):
    r = subprocess.run(cmd, cwd=HUB, capture_output=True, text=True)
    print(cmd[1], r.returncode, (r.stdout + r.stderr).strip()[-300:])
