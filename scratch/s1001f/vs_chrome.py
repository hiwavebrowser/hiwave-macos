"""Board pixel diff (the oracle's own `diff`) of each arm's frame against the stored Chrome frames of the
last quiet board. usage: vs_chrome.py <site> [...]"""
import json, subprocess, sys
REPO = '/Users/petecopeland/Repos/.worktrees/rs-dev-9f49a40'
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
RUN = f'{HUB}/trench/realsite/runs/20261001T1832Z-quiet-dev2b764be'
FR = f'{HUB}/scratch/s1001f/frames'


def diff(a, b):
    r = subprocess.run(['node', f'{REPO}/tools/parity_oracle/realsite.mjs', 'diff', a, b],
                       capture_output=True, text=True, cwd=REPO, timeout=120)
    try:
        j = json.loads(r.stdout.strip().splitlines()[-1])
        return j.get('diff_pct', j.get('diff', j))
    except Exception:
        return (r.stdout + r.stderr)[-200:]


for site in sys.argv[1:]:
    for chrome in ('a', 'b'):
        row = [f'{site:10} chrome-{chrome}']
        for arm, label in (('A1', 'develop'), ('B1', 'fix'), ('A2', 'develop'), ('B2', 'fix')):
            d = diff(f'{RUN}/{site}/chrome-{chrome}.png', f'{FR}/{site}-{arm}.png')
            row.append(f'{label} {d if not isinstance(d, float) else round(d, 2)}')
        print('  '.join(row), flush=True)
