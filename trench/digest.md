# Trench digest — MCP Phase 1 engine exports

One entry per night. Metric: engine export coverage (see `BASELINE.md`).
Noon digest only — no pings between.

---

## 2026-07-29 — night 0 (baseline pinned, no code)

**Metric: 1 of 4 → 1 of 4** (baseline night, nothing claimed)

- Pinned the metric and stop condition in `BASELINE.md` on Pete's go.
- Verified the one existing answerable tool rather than assuming it:
  `crates/hiwave-mcp/smoke.py` passes and asserts `.hero border_box =
  432.0x152.0` from `hiwave_layout` — a value derivable by hand from the
  fixture CSS (400+2×16 × 120+2×16), so the assertion proves the engine
  computed it rather than that the call returned something.
- **Cannot yet answer:** display list, computed style + winning rule, staged
  diff. Three of four.
- **Decisions needed from Pete:** none.

First working night targets `hiwave_display_list`.

---

## 2026-07-30 — night 1 (`hiwave_display_list`)

**Metric: 1 of 4 → 2 of 4**

**Moved no → yes: `hiwave_display_list`.**

### The assertion that proves it

`Engine::export_display_list_json` (new) serves the paint commands the engine
built from the layout tree; `hiwave_display_list` returns them flat and in
paint order. The assertion pairs paint against layout rather than checking the
call succeeded — layout says `.hero`'s border box is 432x152, so paint must be
filling **that** rect with `#08c`:

```python
# #08c is rgb(0, 136, 204) — 0x88 = 136, 0xcc = 204, by hand from the CSS.
hero_fill = [
    c for c in commands
    if c["op"] == "solid_color"
    and c["color"] == {"r": 0, "g": 136, "b": 204, "a": 1.0}
]
assert len(hero_fill) == 1, f"expected exactly one #08c fill, got {len(hero_fill)}"
fill_rect = hero_fill[0]["rect"]
assert fill_rect == {"x": 0.0, "y": 0.0, "width": 432.0, "height": 152.0}, fill_rect
# background paints over the BORDER box, so paint and layout must agree
assert fill_rect["width"] == box["width"], (fill_rect, box)
assert fill_rect["height"] == box["height"], (fill_rect, box)
```

Also asserted, all hand-derivable: canvas < hero < text paint order, `h1`'s
32px size and 700 weight from the cascade, `x=16` from the hero's padding-left,
and one advance per character (the ADVANCE CONTRACT — a `null` there means
paint re-derived its own metrics, which was previously visible only in a trace
log).

Deliberately **not** asserted: baseline `y`, `ascent`, and the advance
*values*. Those are platform font metrics; pinning them would make the gate
report a text-stack difference as an engine regression.

```
$ cargo build -p hiwave-mcp && python3 crates/hiwave-mcp/smoke.py
ok  initialize        {'name': 'hiwave-mcp', 'version': '0.1.0'}
ok  tools/list        ['hiwave_open', 'hiwave_layout', 'hiwave_display_list', 'hiwave_screenshot', 'hiwave_status']
ok  hiwave_layout-before-open  refused: no page loaded — call hiwave_open first
ok  hiwave_display_list-before-open  refused: no page loaded — call hiwave_open first
ok  hiwave_open       {'height': 600, 'loaded': '<inline>', 'width': 800}
ok  hiwave_status     session survives between calls
ok  hiwave_layout     .hero border_box = 432.0x152.0 (content-box: 400+2*16 x 120+2*16)
ok  hiwave_display_list  .hero painted rgb(0,136,204) over 432.0x152.0 at (0.0,0.0) — same rect layout computed
ok  paint order       canvas[0] < hero[1] < text[2]
ok  advance contract  11 advances for 11 chars, font_size=32.0 weight=700 x=16.0
ok  hiwave_screenshot 1440015 bytes at ppm
ok  argument guard    pass either `html` or `path`, not both

PASS: hiwave-mcp serves the engine's computed layout AND its paint commands over MCP
```

**Checked that the gate can go red**, since that is the failure mode this repo
keeps finding: with the fixture's `padding: 16px` changed to `20px`, the paint
rect comes back `440x160` — exactly what hand-derivation predicts — and the
432x152 assertion fails. The number tracks the engine, not the call.

### Two disclosures, both about the runner

1. The trench runs on **Linux**, not macOS. `rustkit-engine` did not compile
   here at all: `create_view` was gated `cfg(not(target_os = "windows"))` while
   its body calls `ViewHostTrait::get_raw_window_handle` and
   `Compositor::create_surface_for_raw_handle`, both of which are
   `cfg(target_os = "macos")` at their definitions.

   ```
   error[E0576]: cannot find method or associated constant `get_raw_window_handle` in trait `ViewHostTrait`
   error[E0599]: no method named `create_surface_for_raw_handle` found for struct `Compositor` in the current scope
   ```

   Fixed in `391ebaa` by narrowing the gate to `cfg(target_os = "macos")`. This
   is a **no-op on both supported platforms** — on macOS both gates match, on
   Windows both exclude — and it is an enabling change, not metric work. It is
   a separate commit so it can be reverted on its own. Flagging it because it
   is outside the trench's remit and Pete may want it handled differently.

2. There was no GPU adapter either (`No suitable GPU adapter found`). Fixed by
   installing `mesa-vulkan-drivers` (lavapipe, software Vulkan) in the
   container. **Environment only — nothing committed.** Any future runner needs
   it or the smoke test cannot start.

### What the engine still cannot answer

- **`hiwave_style` — computed value plus winning rule and origin.** Not
  started, and it is not a serialization job. `compute_style_for_element`
  (lib.rs:1884) sorts matching rules by specificity and then **overwrites
  fields on a `ComputedStyle` struct** in a loop. No provenance survives: there
  is no record of which rule last wrote each property, and no origin tag. The
  computed *value* half is easy; the *winning rule* half needs the cascade
  instrumented to record `property → (selector, specificity, origin)` as it
  applies. Emitting only the value would not meet the bar and would not count.
- **UA-origin properties have no rule to cite.** The user-agent stylesheet is a
  hardcoded Rust `match` on tag name (lib.rs:1949 for `h1`), not a parsed
  sheet. `h1`'s 700 weight is real but no selector produced it, so `origin` for
  those can at best say `user-agent default for <h1>`. Next slice should assert
  against an **author** declaration (e.g. `.hero`'s width), which does have a
  citable rule, and report UA-origin as a named limitation.
- **`hiwave_diff(case, stage, reference)`** — untouched, and correctly so: it
  consumes the other three.
- **Display list gaps, named:** form controls, carets, focus rings, backdrop
  filters, gradient text, and the SVG primitives are emitted as
  `{"op": ..., "modelled": false, "debug": "..."}`. Readable, but the shape is
  not a contract, and nothing in the smoke test covers them. Modelling them is
  cheap and can ride along with a later slice.
- Nothing verifies the display list against a **real page** — only the
  three-command fixture. Clipping, stacking contexts and transforms have export
  arms but zero coverage.

### Tests

`cargo test --workspace` cannot run here: `hiwave-app` and `hiwave-smoke` need
GTK (`gdk-sys` build fails) and `rustkit-media` needs ALSA. Excluding those
three, **612 passed, 0 failed**, plus one pre-existing failure —
`rustkit-layout::probe_normal_line_height_vs_chrome`, which wants `-apple-system`
metrics and reports `0/20 pairs`. Confirmed pre-existing by checking out
`master` and reproducing it there identically; rustkit-layout does not depend
on either crate this night touched.

### Decisions needed from Pete

1. **Is a Linux-verified receipt acceptable, or does this trench need a macOS
   runner?** Tonight's assertions were chosen to be platform-independent —
   geometry, colour, cascade, paint order — and font metrics were deliberately
   left unasserted for exactly this reason. But that is a constraint the loop
   is now working around rather than a property of the metric, and it will bind
   harder on `hiwave_style` (font shorthands) and hardest on `hiwave_diff`,
   which compares against Chrome baselines captured elsewhere. Worth ruling on
   before the diff slice, not after.

Next slice: `hiwave_style` — instrument the cascade to retain
`property → (selector, specificity, origin)`, then assert against an
author-origin declaration with a citable rule.
