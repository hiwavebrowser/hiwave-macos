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

---

## 2026-07-31 — night 2 (`hiwave_style`)

**Metric: 2 of 4 → 3 of 4**

**Moved no → yes: `hiwave_style`.**

### The assertion that proves it

The cascade now records provenance from **inside** the loop that already
applies declarations (`compute_style_for_element`), not from a second pass
that re-walks the rules — a separate recorder could disagree with the
cascade, and a tool that disagrees with the engine is worse than no tool.
The winner is derived from *application order* (the declaration that wrote
the field last), so it cannot drift from the value that survived.

The fixture gained one line, and it is load-bearing:

```css
.hero { width: 400px; height: 120px; padding: 16px; background: #08c }
div   { width: 100px }     /* later in source, LOWER specificity — must lose */
```

So the assertion is not "a value came back" — it is that the engine names
the rule it chose *and* the rule it rejected:

```python
assert hero_style["computed"]["width"] == "400px", hero_style["computed"]
# THE ASSERTION hiwave_style EXISTS FOR: (0,1,0) beats (0,0,1) even though
# `div` came second in the sheet.
assert width["winner"]["selector"] == ".hero", width["winner"]
assert width["winner"]["specificity"] == [0, 1, 0], width["winner"]
assert width["winner"]["value"] == "400px", width["winner"]
assert width["winner"]["origin"] == "author", width["winner"]
# And the rule it BEAT is reported rather than dropped.
assert len(width["overridden"]) == 1, width["overridden"]
loser = width["overridden"][0]
assert loser["selector"] == "div", loser
assert loser["specificity"] == [0, 0, 1], loser
assert loser["value"] == "100px", loser
```

Plus two more, both hand-derivable. The first pins the *computed* half — the
value is read off the `ComputedStyle` the cascade produced, not echoed back
from declaration text, and **no rule in the fixture spells `padding-left`**:

```python
assert hero_style["computed"]["padding-left"] == "16px", hero_style["computed"]
pad_left = next(d for d in hero_style["declared"] if d["property"] == "padding-left")
assert pad_left["winner"] is None, pad_left      # nothing declared it...
assert pad_left["computed"] == "16px", pad_left  # ...yet it computed to 16px
assert any(d["property"] == "padding" and d["winner"]["value"] == "16px"
           for d in hero_style["declared"]), hero_style["declared"]
```

The second pins the **origin split**, on the same element pair a real
debugging session would hit — `h1`'s bold is genuinely the UA sheet's, and
its size is genuinely the author's:

```python
assert h1_el["computed"]["font-weight"] == "700", h1_el["computed"]
weight = next(d for d in h1_el["declared"] if d["property"] == "font-weight")
assert weight["winner"] is None, weight                      # UA: no rule to cite
assert weight["origin"] == "user-agent-or-initial", weight
size = next(d for d in h1_el["declared"] if d["property"] == "font-size")
assert size["computed"] == "32px", size
assert size["winner"]["selector"] == "h1", size["winner"]    # author: cites one
assert size["origin"] == "author", size
```

```
$ cargo build -p hiwave-mcp && python3 crates/hiwave-mcp/smoke.py
ok  initialize        {'name': 'hiwave-mcp', 'version': '0.1.0'}
ok  tools/list        ['hiwave_open', 'hiwave_layout', 'hiwave_display_list', 'hiwave_style', 'hiwave_screenshot', 'hiwave_status']
ok  hiwave_layout-before-open  refused: no page loaded — call hiwave_open first
ok  hiwave_display_list-before-open  refused: no page loaded — call hiwave_open first
ok  hiwave_style-before-open  refused: no page loaded — call hiwave_open first
ok  hiwave_open       {'height': 600, 'loaded': '<inline>', 'width': 800}
ok  hiwave_status     session survives between calls
ok  hiwave_layout     .hero border_box = 432.0x152.0 (content-box: 400+2*16 x 120+2*16)
ok  hiwave_display_list  .hero painted rgb(0,136,204) over 432.0x152.0 at (0.0,0.0) — same rect layout computed
ok  paint order       canvas[0] < hero[1] < text[2]
ok  advance contract  11 advances for 11 chars, font_size=32.0 weight=700 x=16.0
ok  hiwave_style      width=400px won by .hero [0, 1, 0] over div [0, 0, 1] (later in source, lower specificity)
ok  computed expansion padding-left=16px, winner=None — no rule spells it; expanded from `padding: 16px`
ok  origin split      h1 font-weight=700 winner=None (UA, no rule to cite); font-size=32px winner=h1 (author)
ok  selector guard    refused 'div p'
ok  hiwave_screenshot 1440015 bytes at ppm
ok  argument guard    pass either `html` or `path`, not both

PASS: hiwave-mcp serves the engine's computed layout, its paint commands, AND the cascade behind them over MCP
```

**Checked that the gate can go red.** Perturbed the cascade's sort to source
order only (specificity ignored), rebuilt, and queried:

```
computed width : 100px
winner         : div [0, 0, 1] 100px
overridden     : [('.hero', [0, 1, 0], '400px')]
```

All four winner/loser assertions fail, and the layout assertion fails with
them — the tool reports the inverted cascade rather than the expected one,
so it tracks the engine and not the call. Perturbation reverted; `grep -c
RED-CHECK` on the committed tree returns 0.

### A real finding, surfaced by building the tool

**`!important` is parsed but the cascade does not honour it.**
`rustkit-cssparser` sets `Declaration.important` correctly (it has its own
passing test), and `rustkit_css::Declaration` carries the flag through — but
`compute_style_for_element` orders matching rules by specificity and source
index *only*. Nothing reads `important`. So `div { width: 100px !important }`
loses to `.hero { width: 400px }`, where every browser gives 100px.

This is exactly the "parsed but dead" class the slice was aimed at, and it
was invisible before because the surviving value looks unremarkable. I did
**not** fix it — that is cascade surgery, not export work, and fixing it
would change rendering across the whole parity corpus on a night whose
metric is export coverage. `hiwave_style` reports the flag, so an
`important: true` declaration sitting in `overridden` is now visible as an
engine bug rather than a reporting artefact. Flagging for Pete below.

### What the engine still cannot answer

- **UA-origin properties have no rule to cite** — named by night 1, and now
  reported explicitly rather than silently: the UA sheet is a hardcoded Rust
  `match` on tag name (lib.rs, the `"h1" =>` arm), not parsed rules. `h1`'s
  700 weight is real, but no selector produced it, so it comes back
  `"winner": null`, `"origin": "user-agent-or-initial"` and is asserted that
  way. Note the honest ceiling: this bucket cannot distinguish "the UA sheet
  set it" from "nobody set it and this is the initial value", because at
  this layer those are the same thing. Separating them means turning the
  match into a parsed UA stylesheet — a genuine engine change, not an export
  one, and out of scope for a night whose metric is export coverage.
- **Computed values cover 15 longhands, not the full set.** `width`,
  `height`, the four `padding-*` and four `margin-*`, `font-size`,
  `font-weight`, `color`, `background-color`, `display`. Anything else
  returns `null` and the supported list ships in the payload's `limits`.
  Notably absent: `line-height`, `font-family`, `text-align`, `border-*`,
  `position`, and every gradient/background-layer field. The gap is honest
  rather than guessed — a wrong computed value in a tool built to
  adjudicate computed values is the worst failure available to it.
- **Shorthands are reported as authored, not expanded into longhands — and
  this is the one place the output can mislead.** The `declared` list for
  `.hero` carries `padding` (winner `.hero`, value `16px`) *and*
  `padding-left` with `"winner": null`. That null is literally true — no
  rule spells `padding-left` — but a careless reader takes it as "nothing
  set this", when `padding: 16px` plainly did. "Which rule set
  `padding-left`" is therefore **not answerable**; only "what did it compute
  to" and "which rules set `padding`". Fixing it means teaching the recorder
  each shorthand's expansion so the longhand inherits its shorthand's
  provenance. Tractable, and the obvious next increment if cascade
  provenance needs to go deeper — I did not do it tonight because it is a
  second, separately-testable slice, not a finishing touch on this one.
- **Selector queries are simple selectors only** (`tag`, `.class`, `#id`,
  `tag.class`). Descendant/sibling/attribute/pseudo queries are **refused**,
  not approximated, because the trace does not keep the tree context the
  cascade had. Two unit tests pin both the matching and the refusal.
- **`hiwave_diff(case, stage, reference)`** — untouched, and correctly so:
  it is last because it consumes the other three.
- Nothing exercises `hiwave_style` against a **real page**. Cascade depth,
  inheritance chains, CSS variables and `@media` blocks all flow through the
  recorder but have zero coverage beyond the four-rule fixture.

### Tests

`cargo test --workspace --no-fail-fast`, excluding `hiwave-app` and
`hiwave-smoke` (need GTK — `gdk-sys` build fails) and `rustkit-media` (needs
ALSA): **899 passed, 1 failed**. The one failure is
`rustkit-layout::probe_normal_line_height_vs_chrome`, pre-existing and
unrelated — re-confirmed this night by `git stash`ing the change and
reproducing it identically (`0/20 pairs`) on the untouched tree.
`rustkit-layout` does not depend on either crate this night touched.

`cargo build --release -p parity-capture` — the only thing CI actually
builds — **finishes clean**, so nothing here slips the workspace gate.

Two housekeeping notes, neither a change I made: the runner needed
`mesa-vulkan-drivers` installed again (fresh container; environment only,
nothing committed — without it the engine has no GPU adapter and the smoke
test cannot start). And `cargo fmt --check` reports diffs in these two
crates — 16 of them **already present on HEAD before this night**, 6 from
tonight's code. I did not run `cargo fmt`, because it would reformat a lot
of unrelated code and CI gates neither fmt nor clippy. Say the word if you
want the crates formatted as their own commit.

### Decisions needed from Pete

1. **`!important` is dead in the cascade — fix it, or record it and move
   on?** It is a real correctness bug (see above) and it is now *visible*
   rather than merely present. But honouring it will move rendering on any
   parity case that uses `!important`, so it is a parity-corpus event, not a
   trench-slice event. My read: file it, do not fix it inside this loop.
2. **Night 1's open question still stands** and is now the last cheap moment
   to answer it: is a Linux-verified receipt acceptable, or does this trench
   need a macOS runner? Tonight's assertions stayed platform-independent
   (specificity, cascade order, shorthand expansion) and that was
   comfortable. It will not be for `hiwave_diff`, which compares against
   Chrome baselines captured elsewhere — that is the slice where the runner
   platform stops being a workaround and starts being the answer.

Next slice: `hiwave_diff` — LAST by design, and it now has all three inputs
it consumes. The first question to settle is not code: it is what `reference`
means on a runner that is not the one the baselines came from.
