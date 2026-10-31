# CSS property census (dev-tools T13)

    cargo run --profile parity -p css-census

From a clean clone that builds `parity-capture`, installs the oracle's node
modules if they are missing, renders every fixture in `parity-capture` and in
the campaign's Chromium (143.0.7499.4), and writes
`docs/census/CSS_PROPERTY_CENSUS_<date>.md` and `.json`. About 90 seconds after
the build. The run refuses a different Chromium, an arm reader that lost
`apply_style_property`, and a sentinel (`background-color`, `width`, `color`)
it cannot tell from its control.

`properties.json` is the list: name, context (a page in `src/lib.rs`), `base`
declarations both arms need, 1 to 3 non-initial values, `nonvisual` for a
property that paints nothing in a static frame, `pseudo` to declare it on one.

Every value is captured with a control that lacks the declaration, in both
engines. Each page's own script prints `getComputedStyle` and the rects of every
`[id]` as one `CENSUS:` console line, so both arms read through the same code.

- **parse**: an arm in `apply_style_property` or `logical_to_physical`, or the
  engine's page moved when the declaration was added.
- **computed**: the read equals Chrome's, numbers within 0.05.
- **effect**: Chrome changed (3+ pixels, or a rect), all rects are within 1 px of
  Chrome's, off probes are within the controls' disagreement plus a tenth of
  Chrome's change, and the engine changed at least half as many pixels as Chrome.
  A probe every 4 px agrees when either frame's colour is within 16 per channel in
  the other's 3x3, which forgives antialiasing and a 1 px seat; the pixel count
  catches the thin line that forgiveness would hide.

A property is its worst measured value. `--only a,b` prints JSON for those rows
and writes no report; `--parity-capture <path>` skips the build.
