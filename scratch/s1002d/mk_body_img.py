"""Body for the image-loader-routing PR: pr-img-head.md with the fail-first lines, the campaign receipt,
the A/B rows (ab-img.txt) and the movers paragraph (movers-img.md). Writes pr-img-final.md."""
D = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/s1002d'
head = open(f'{D}/pr-img-head.md').read()
ff = open(f'{D}/failfirst-img.txt').read()
keep = []
for line in ff.splitlines():
    s = line.strip()
    if s.startswith(('assertion', 'left:', 'right:', 'test result')) or 'panicked at' in s:
        if s not in keep:
            keep.append(s)
assert 'FAILFIRST' in head and keep
head = head.replace('FAILFIRST', '\n'.join(keep))
receipt = open(f'{D}/receipt-img.md').read()
ab = ''.join(l for l in open(f'{D}/ab-img.txt') if l.strip() != 'DONE')
movers = open(f'{D}/movers-img.md').read()
body = f"""{head}
## Campaign receipt

Arms: develop **60f7d39** vs fix **ef58355**, both release binaries built in this session with every workspace source touched first. The branch's base is develop e60ba68, which is 60f7d39 plus #436 (a cascade-lane change to the selector match key), so the fix arm carries #436 and the develop arm does not; I did not build a third arm at e60ba68.

{receipt}
The campaign's pages load local files and `data:` images, so this says the decode and cache path is unchanged, not that the network route is right; the tests and the live A/B are for that.

## Real sites

RustKit frames, develop / fix / develop / fix on each of the board's 20 live URLs at 1280x800 (`scratch/s1001g/ab.py` on the hub branch), load 12 to 14 with the other lanes building. "Within" is a site's own variance between two captures of one arm; "across" is the four develop-fix pairs; the share of pixels where any channel differs by more than 8. `parity-capture` installs no shield, so this measures the Referer, the user agent and the cookie setting on image requests, not blocking.

```
{ab}```

{movers}
🤖 Generated with [Claude Code](https://claude.com/claude-code)
"""
open(f'{D}/pr-img-final.md', 'w').write(body)
print(len(body))
