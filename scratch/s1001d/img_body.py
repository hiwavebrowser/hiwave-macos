"""Write the PR body for atlas/rs-img-border-radius from the two campaign arms. usage: img_body.py -> pr-body-img.md"""
import json
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch'
dev, fix = 'rsc-wip', 'imgr-a739e7d'


def load(arm, scope):
    j = json.load(open(f'{HUB}/s0929e/camp-{arm}/{scope}.json'))
    rs = j['results']
    return j.get('timestamp', '')[:19], {r['case_id']: r['diff_pct'] for r in rs}, sum(1 for r in rs if r.get('passed'))


summary, tables = [], {}
for scope in ('all', 'builtins', 'micro'):
    ta, a, pa = load(dev, scope)
    tb, b, pb = load(fix, scope)
    same = sum(1 for k in a if a[k] == b.get(k))
    summary.append(f'{scope + ":":9} develop {ta} {pa:2}/{len(a):<2} avg {sum(a.values())/len(a):.4f} | '
                   f'fix {tb} {pb:2}/{len(b):<2} avg {sum(b.values())/len(b):.4f} | {same}/{len(a)} identical')
    tables[scope] = '\n'.join(
        f'| {k} | {a[k]:.4f} | {b[k]:.4f} |' + ('' if a[k] == b[k] else ' moved') for k in sorted(a))
ra = open(f'{HUB}/s0929e/camp-{dev}/ratchet.txt').read().splitlines()
rb = open(f'{HUB}/s0929e/camp-{fix}/ratchet.txt').read().splitlines()
assert ra == rb, 'ratchet differs'
body = f'''## What

`border-radius` on an `<img>` did not round the image, only the image's (usually empty) background. `img {{ border-radius: 50% }}` is the usual way to write an avatar, with no `overflow: hidden` box around it, and it painted a square photo.

Follow-up to #407, which made the renderer apply rounded clips to textured quads and named this case as not included.

## Change

**rustkit-layout, `render_replaced_content`:** replaced content is trimmed to the content edge curve (CSS Backgrounds 3 §5.3): the border radius inset by the border and padding beside each corner (`CornerRadius::inset`, the same rule the overflow clip uses for the padding edge). When any corner of that curve is round, the image command is wrapped in `PushClipRounded {{ content box, content radius }}` / `PopClip`. An image with no radius, or one whose border and padding swallow it, emits exactly the commands it did.

## Not in this PR

- Other replaced content (form controls, inline SVG, video) is not clipped to the curve.

## Tests

**Fail first:** `a_rounded_img_clips_its_image_to_the_content_edge_curve` reads the display list as text, so it compiles on either tree. Inserted into develop c6b4841's `windows_engine_pins` and run there: **1 failed, 19 passed** (the command before the image is the page background, not a rounded clip; worktree restored and clean after). On this branch it passes. It pins: a `50%` radius on a 100x60 image gives a 50x30 clip opened just before the image and closed just after; a 30px radius under a 5px border and 5px padding gives 20px; an 8px radius under the same border and padding, and no radius, give no clip.

Suites at head a739e7d: **rustkit-layout 583/583.** rustkit-engine: the two pins touched here pass (`a_rounded_img_clips...`, `a_shadow_carries...`), `windows_engine_pins` 20/20 without the headless feature. The full engine suite was not run locally (three lanes share this machine); CI is that evidence.

## Campaign receipt

Arms: develop's engine vs fix **a739e7d**. The develop arm is the release binary of **fc64b0e**, #407's head: develop c6b4841 is that commit plus #405 and #406, which change nothing under `crates/`, `fixtures/` or `baselines/` (`git diff --stat fc64b0e origin/develop`). Both binaries were built in this session with every workspace source touched first.

```
{chr(10).join(summary)}
```

`ratchet_local.py` (CI's Gate A / Gate B / ratchet): output identical on both arms, line for line.

Expected: no campaign case puts a radius on an `<img>`.

<details><summary>Per-case diff_pct (all 26), develop vs fix a739e7d</summary>

| case | develop | fix |
|---|---|---|
{tables['all']}

</details>

<details><summary>Per-case diff_pct (builtins 5 and micro 13)</summary>

| case | develop | fix |
|---|---|---|
{tables['builtins']}
{tables['micro']}

</details>

## Frames

#407's fixture (hub `scratch/s1001d/shadow-clip.html`), third box of the second row: `<img style="border-radius: 30px">`, 120x120, against pinned Chrome 148.

| arm | pixels of that box that differ from Chrome | of the whole 1280x340 region |
|---|---|---|
| develop (fc64b0e) | 852 | 9,661 (2.220%) |
| fix a739e7d | 98 | 8,907 (2.047%) |

The region's drop is exactly that box's (754 pixels); nothing else in the fixture changed.

## Real sites

No live-site A/B was taken for this change (session cap), and **no points are claimed**. The next quiet daily board is the measurement.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
'''
open(f'{HUB}/s1001d/pr-body-img.md', 'w').write(body)
print('\n'.join(summary))
print(len(body), 'chars')
