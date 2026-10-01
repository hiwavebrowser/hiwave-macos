## What

`border-radius` on an `<img>` did not round the image, only the image's (usually empty) background. `img { border-radius: 50% }` is the usual way to write an avatar, with no `overflow: hidden` box around it, and it painted a square photo.

Follow-up to #407, which made the renderer apply rounded clips to textured quads and named this case as not included.

## Change

**rustkit-layout, `render_replaced_content`:** replaced content is trimmed to the content edge curve (CSS Backgrounds 3 §5.3): the border radius inset by the border and padding beside each corner (`CornerRadius::inset`, the same rule the overflow clip uses for the padding edge). When any corner of that curve is round, the image command is wrapped in `PushClipRounded { content box, content radius }` / `PopClip`. An image with no radius, or one whose border and padding swallow it, emits exactly the commands it did.

## Not in this PR

- Other replaced content (form controls, inline SVG, video) is not clipped to the curve.

## Tests

**Fail first:** `a_rounded_img_clips_its_image_to_the_content_edge_curve` reads the display list as text, so it compiles on either tree. Inserted into develop c6b4841's `windows_engine_pins` and run there: **1 failed, 19 passed** (the command before the image is the page background, not a rounded clip; worktree restored and clean after). On this branch it passes. It pins: a `50%` radius on a 100x60 image gives a 50x30 clip opened just before the image and closed just after; a 30px radius under a 5px border and 5px padding gives 20px; an 8px radius under the same border and padding, and no radius, give no clip.

Suites at head a739e7d: **rustkit-layout 583/583.** rustkit-engine: the two pins touched here pass (`a_rounded_img_clips...`, `a_shadow_carries...`), `windows_engine_pins` 20/20 without the headless feature. The full engine suite was not run locally (three lanes share this machine); CI is that evidence.

## Campaign receipt

Arms: develop's engine vs fix **a739e7d**. The develop arm is the release binary of **fc64b0e**, #407's head: develop c6b4841 is that commit plus #405 and #406, which change nothing under `crates/`, `fixtures/` or `baselines/` (`git diff --stat fc64b0e origin/develop`). Both binaries were built in this session with every workspace source touched first.

```
all:      develop 2026-10-01T10:09:53 26/26 avg 1.1271 | fix 2026-10-01T11:10:26 26/26 avg 1.1271 | 26/26 identical
builtins: develop 2026-10-01T10:05:26  5/5  avg 1.9072 | fix 2026-10-01T11:06:49  5/5  avg 1.9072 | 5/5 identical
micro:    develop 2026-10-01T10:04:35 13/13 avg 0.6550 | fix 2026-10-01T11:06:12 13/13 avg 0.6550 | 13/13 identical
```

`ratchet_local.py` (CI's Gate A / Gate B / ratchet): output identical on both arms, line for line.

Expected: no campaign case puts a radius on an `<img>`.

<details><summary>Per-case diff_pct (all 26), develop vs fix a739e7d</summary>

| case | develop | fix |
|---|---|---|
| about | 3.6742 | 3.6742 |
| article-typography | 4.7857 | 4.7857 |
| backgrounds | 1.2163 | 1.2163 |
| bg-pure | 0.0000 | 0.0000 |
| bg-solid | 0.2106 | 0.2106 |
| card-grid | 1.3074 | 1.3074 |
| chrome_rustkit | 1.1234 | 1.1234 |
| combinators | 0.6547 | 0.6547 |
| css-selectors | 1.3819 | 1.3819 |
| flex-positioning | 0.6327 | 0.6327 |
| form-controls | 3.2405 | 3.2405 |
| form-elements | 0.9535 | 0.9535 |
| gpu-gradient-regression | 0.3903 | 0.3903 |
| gradient-backgrounds | 1.0133 | 1.0133 |
| gradient-no-radius | 0.5204 | 0.5204 |
| gradient-radius-only | 0.3429 | 0.3429 |
| gradients | 0.1412 | 0.1412 |
| image-gallery | 0.5217 | 0.5217 |
| images-intrinsic | 0.3267 | 0.3267 |
| new_tab | 1.5685 | 1.5685 |
| pseudo-classes | 0.4341 | 0.4341 |
| rounded-corners | 0.4354 | 0.4354 |
| settings | 2.0919 | 2.0919 |
| shelf | 1.0781 | 1.0781 |
| specificity | 0.6015 | 0.6015 |
| sticky-scroll | 0.6575 | 0.6575 |

</details>

<details><summary>Per-case diff_pct (builtins 5 and micro 13)</summary>

| case | develop | fix |
|---|---|---|
| about | 3.6742 | 3.6742 |
| chrome_rustkit | 1.1234 | 1.1234 |
| new_tab | 1.5685 | 1.5685 |
| settings | 2.0919 | 2.0919 |
| shelf | 1.0781 | 1.0781 |
| backgrounds | 1.2163 | 1.2163 |
| bg-pure | 0.0000 | 0.0000 |
| bg-solid | 0.2106 | 0.2106 |
| combinators | 0.6547 | 0.6547 |
| form-controls | 3.2405 | 3.2405 |
| gpu-gradient-regression | 0.3903 | 0.3903 |
| gradient-no-radius | 0.5204 | 0.5204 |
| gradient-radius-only | 0.3429 | 0.3429 |
| gradients | 0.1412 | 0.1412 |
| images-intrinsic | 0.3267 | 0.3267 |
| pseudo-classes | 0.4341 | 0.4341 |
| rounded-corners | 0.4354 | 0.4354 |
| specificity | 0.6015 | 0.6015 |

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
