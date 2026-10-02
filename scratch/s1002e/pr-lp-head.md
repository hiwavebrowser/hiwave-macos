## What

A `url()` background image at a length position sat at its box's corner. `DisplayCommand::BackgroundImage` carries the position as two shares of the free space (0 to 1), and layout turned a length into share 0 with a comment that paint would handle it; paint never saw the length. So `background-position: 10px 20px`, every sprite offset (`-20px -40px`) and `right 10px bottom 5px` all drew the image at the top left (or flush on the far edge). Gradients were right already: their rect is computed in layout, where the length is known.

This is the second of the background-image PRs Atlas ordered on 2026-10-02 (fetch, length positions, SVG backgrounds). It is independent of the fetch (#FETCHPR): a `data:` image shows it on develop today, and once backgrounds are fetched it decides which part of a sprite sheet shows.

## Change

- `rustkit-css`: `BackgroundPositionValue::FromEnd(px)` (inward from the far edge), and `share_and_offset()`, the position as a share of the free space plus a pixel offset.
- `rustkit-engine`: `parse_background_position` keeps the offset of `right <offset>` / `bottom <offset>` (`FromEnd` for a length, `1 - p` for a percentage); it used to put the image on the edge. The display-list JSON dump prints the offset.
- `rustkit-layout`: the command gets `offset: (f32, f32)`; `convert_background_position` fills it.
- `rustkit-renderer`: `draw_background_image` adds the offset to the start position. Tiling and the clip to the box are unchanged, so a negative offset shows the right part of the sheet.

## Tests

- `rounded_paint_frame_tests::a_url_background_sits_at_its_length_position` (`rustkit-engine`, headless, reads the captured frame): three blue boxes with a red image at `10px 30px`, `right 10px bottom 5px` and `-10px -10px`; seven pixels, inside and outside each image.
- `a_background_position_reads_its_keywords_by_axis`: `right 5px bottom 10px` and `right 25% bottom` (the old expectation pinned the dropped offset).

Fail-first: the frame test put into develop's module (f0d5fa2), develop's code otherwise untouched (`scratch/s1002e/failfirst.py lp` on the hub branch):

```
FAILFIRST
```

On the branch:

```
ONBRANCH
```

