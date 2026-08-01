# `hiwave_diff` cases

A case is a page plus one or more references. `hiwave_diff(case, stage,
reference)` runs `page.html` in a fresh headless engine, exports one stage, and
reports every field where the engine disagrees with the reference — with both
values.

```
cases/<case>/page.html                  the page, run as-is
cases/<case>/<reference>.<stage>.json   what the engine SHOULD compute
```

`<stage>` is `layout` or `display_list`. Both are text artefacts, which is the
point: the answer is the same on any machine, and does not require trusting a
GPU capture on a runner whose captures are not trusted.

## Reference format

```json
{
  "case": "hero",
  "stage": "layout",
  "kind": "expectations",
  "origin": "hand-derived from the CSS in page.html",
  "viewport": { "width": 800, "height": 600 },
  "expect": [
    { "path": "root.children[0].children[0].border_box.width",
      "value": 432.0,
      "why": "width 400 + padding-left 16 + padding-right 16" }
  ]
}
```

- **`kind`** — only `expectations` is implemented. The plan also calls for a
  `capture` kind (a full committed macOS export, diffed structurally) to make
  this the port-verification receipt for Windows and Linux seats. That is not
  built yet, and a reference declaring it is refused rather than misread.
- **`origin`** — where the numbers came from. It is reported back in the tool's
  result, because a reference whose provenance is unknown cannot adjudicate
  anything.
- **`viewport`** — stated, not inherited. Layout depends on it, so a reference
  that did not pin it would silently mean something different at another size.
- **`why`** — the derivation, in the file, next to the number. If a value
  cannot be justified in one line from the page's CSS, it does not belong in a
  reference.
- **`tolerance`** — optional, absolute. Omitted means exact.

## What must NOT go in a reference

Font metrics: baseline `y`, `ascent`, glyph advances, and any height that falls
out of a line box. They legitimately differ by text stack, so pinning them
makes the tool report a platform difference as an engine regression — the
failure mode this whole trench exists to avoid. Assert geometry the CSS
determines, colour, paint order, and cascade outcomes.

## Cases

| Case | What it pins |
|---|---|
| `hero` | The agreeing path. Box geometry from `width`/`height`/`padding`, and the paint commands over the same rectangle. |
| `important-width` | The disagreeing path, on a **real engine bug**: `!important` is parsed and carried but the cascade does not honour it, so the engine computes 400px where every browser computes 100px. See that case's README note before "fixing" the reference. |
