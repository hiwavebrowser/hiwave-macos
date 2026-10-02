"""Assemble the inputs for fill_body.py for the hex PR: frames rows from the four A/B chunks (the retry
chunk wins for a site it captured), fixture table, suites, movers."""
import re
D = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/s1002c'
rows = {}
for f in ('abhex1.txt', 'abhex2.txt', 'abhex3.txt', 'abhex4.txt'):
    for line in open(f'{D}/{f}'):
        m = re.match(r'(\w+)\s+within develop', line)
        if m:
            site = m.group(1)
            # keep a row with more captures over one with fewer
            bad = line.count('nan')
            if site not in rows or bad < rows[site].count('nan'):
                rows[site] = line.rstrip()
open(f'{D}/frames.txt', 'w').write('\n'.join(rows.values()) + '\n')
print(len(rows), 'sites')
ff = open(f'{D}/failfirst.txt').read()
i = ff.index('---- ')
j = ff.index('note: run with')
open(f'{D}/failfirst-short.txt', 'w').write(ff[i:j].strip() + '\n')
t = open(f'{D}/pr-hex.md').read().replace('__FIXTURE__', open(f'{D}/fixture.txt').read().strip())
open(f'{D}/pr-hex-f.md', 'w').write(t)
