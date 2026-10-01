"""PR body for the logical-borders fix from two campaign arms (all + builtins + ratchet).
usage: fill_logical.py <dev-arm> <fix-arm> <head> <facebook.txt>"""
import json, sys
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch'
dev, fix, head, fb = sys.argv[1:5]


def load(arm, scope):
    j = json.load(open(f'{HUB}/s0929e/camp-{arm}/{scope}.json'))
    return j.get('timestamp', '')[:19], {r['case_id']: r['diff_pct'] for r in j['results']}, sum(
        1 for r in j['results'] if r.get('passed'))


summary, table = [], ''
for scope in ('all', 'builtins'):
    ta, a, pa = load(dev, scope)
    tb, b, pb = load(fix, scope)
    same = sum(1 for k in a if a[k] == b.get(k))
    summary.append(f'{scope + ":":9} develop {ta} {pa:2}/{len(a):<2} avg {sum(a.values())/len(a):.4f} | '
                   f'fix {tb} {pb:2}/{len(b):<2} avg {sum(b.values())/len(b):.4f} | {same}/{len(a)} identical')
    if scope == 'all':
        table = '\n'.join(f'| {k} | {a[k]:.4f} | {b[k]:.4f} |' + ('' if a[k] == b[k] else ' moved') for k in sorted(a))
ra = open(f'{HUB}/s0929e/camp-{dev}/ratchet.txt').read().splitlines()
rb = open(f'{HUB}/s0929e/camp-{fix}/ratchet.txt').read().splitlines()
moved = [(x.strip(), y.strip()) for x, y in zip(ra, rb) if x != y]
if not moved and len(ra) == len(rb):
    ratchet = '`ratchet_local.py` (CI\'s Gate A / Gate B / ratchet): output identical on both arms, line for line.'
else:
    ratchet = ('`ratchet_local.py`: the lines that differ between the arms, develop then fix:\n\n```\n'
               + '\n'.join(f'dev: {x}\nfix: {y}' for x, y in moved) + '\n```')
body = f'''## What

The flow-relative border properties had no arms in the engine, so any declaration written with them was dropped: `border-inline-start`, `border-block-end-width`, `border-inline-color`, `border-inline`, and the rest of the family. StyleX and Tailwind v4 emit them.

Found on facebook while taking #401's live-site frames: its login inputs painted a top and a bottom border and no left or right one.

## Change

One file, `rustkit-engine`. `logical_to_physical` (which already maps the margin, padding and inset names) gains:

- the per-side shorthands: `border-inline-start|end`, `border-block-start|end` -> `border-left|right|top|bottom`;
- the per-side longhands for width, style and color (12 names);
- the two-value forms `border-inline-width|style|color` and `border-block-width|style|color` (`start end`; one value sets both);
- `border-inline` and `border-block`, whose whole value goes to both sides (a new `Both` mapping).

A two-value logical shorthand is now split at top-level whitespace instead of every space, so `rgb(1, 2, 3)` is one value. That also fixes the existing `margin-inline: calc(1px + 2px)`, which was dropped as three values.

Horizontal-tb, ltr only, as for the names already mapped.

## Tests

`logical_property_tests::logical_border_properties_map_to_physical_sides`: laid-out border widths for `border-inline-width: 2px 5px`, `border-block-end-width`, `border-inline-start: 3px solid red`, `border-block: 4px solid rgb(1, 2, 3)` and `border-inline: 6px solid blue`; computed styles and colors for the style and color forms, including two colors that each contain spaces. **Fails on develop f16ad4e's code** (run with only the test added: borders `(0, 0, 0, 0)` where `(0, 5, 7, 2)` is expected) and passes with the change. The existing `logical_margin_padding_and_inset_map_to_physical_sides` still passes. The full engine suite was not run locally (machine load 15 to 17); CI is that evidence.

## facebook

{open(fb).read().strip()}

## Campaign receipt

Arms: develop **f16ad4e** (this branch's base) vs fix **{head}**, both release binaries built in this session with every workspace source touched first.

```
{chr(10).join(summary)}
```

{ratchet}

<details><summary>Per-case diff_pct (all 26), develop f16ad4e vs fix {head}</summary>

| case | develop | fix |
|---|---|---|
{table}

</details>

🤖 Generated with [Claude Code](https://claude.com/claude-code)
'''
open(f'{HUB}/s1001c/pr-body-logical.md', 'w').write(body)
print('\n'.join(summary))
print(ratchet[:800])
