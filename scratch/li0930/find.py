"""usage: find.py <regex> [subdir=crates] [maxhits=60] — search .rs files in the fix worktree."""
import pathlib, re, sys
WT = '/Users/petecopeland/Repos/.worktrees/rs-cascade-layers'
pat = re.compile(sys.argv[1])
sub = sys.argv[2] if len(sys.argv) > 2 else 'crates'
mx = int(sys.argv[3]) if len(sys.argv) > 3 else 60
n = 0
for p in sorted(pathlib.Path(WT, sub).rglob('*.rs')):
    for i, l in enumerate(p.read_text(errors='replace').splitlines(), 1):
        if pat.search(l):
            print(f'{p.relative_to(WT)}:{i}: {l.strip()[:160]}'); n += 1
            if n >= mx:
                sys.exit()
