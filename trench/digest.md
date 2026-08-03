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

---

## 2026-08-01 — night 3 (`hiwave_diff`)

**Metric: 3 of 4 → 4 of 4**

**Moved no → yes: `hiwave_diff`.** The trench's stop condition is met.

### The assertion that proves it

`hiwave_diff(case, stage, reference)` runs a committed case in its **own**
engine, exports one stage, and reports every field where the engine disagrees
with a committed reference — with both values. Stages are `layout` and
`display_list`: text artefacts, so the answer is the same on any machine and
does not depend on trusting a GPU capture. Cases live in
`crates/hiwave-mcp/cases/<case>/`, references in `<reference>.<stage>.json`,
with the derivation of every number written next to it.

A comparison tool that can only agree is not a comparison tool, so it is
asserted in **both directions**.

**Agreeing**, and guarded against passing vacuously — an empty `expect` array
would also report `agrees: true`, so the smoke test reads the reference file
and pins both its length and the number the whole crate is built on:

```python
hero_ref = json.loads((CASES / "hero" / "spec.layout.json").read_text())
by_path = {e["path"]: e["value"] for e in hero_ref["expect"]}
assert by_path["root.children[0].children[0].border_box.width"] == 432.0, by_path
assert by_path["root.children[0].children[0].border_box.height"] == 152.0, by_path

d, error = client.tool("hiwave_diff", case="hero", stage="layout", reference="spec")
assert d["checked"] == len(hero_expect) >= 12, (d["checked"], len(hero_expect))
assert d["differences"] == 0 and d["agrees"] is True, d
assert d["disagreements"] == [], d["disagreements"]
```

**Disagreeing** — and on a *real* engine bug rather than a contrived one. This
is the assertion `hiwave_diff` exists for. The case is

```css
.hero { width: 400px; height: 120px }
div   { width: 100px !important }
```

`!important` outranks a normal declaration of the same origin regardless of
specificity, so the width is 100px in every browser. RustKit parses the flag,
carries it, and never reads it (night 2's finding), so it computes 400px. The
diff must report exactly that, at both paths width reaches, and must leave the
two uncontested values in the same reference alone:

```python
d, error = client.tool("hiwave_diff", case="important-width",
                       stage="layout", reference="spec")
assert d["agrees"] is False, d
assert d["checked"] == 4 and d["differences"] == 2, d
widths = {x["path"]: x for x in d["disagreements"]}
border = widths["root.children[0].children[0].border_box.width"]
assert border["expected"] == 100.0 and border["actual"] == 400.0, border
content = widths["root.children[0].children[0].content_rect.width"]
assert content["expected"] == 100.0 and content["actual"] == 400.0, content
# ...and the two uncontested values in the same reference still agree, so
# the disagreement is attributed to one property rather than "everything".
assert "root.children[0].children[0].border_box.height" not in widths, d
assert "root.children[0].children[0].border_box.x" not in widths, d
```

Also asserted: the `display_list` stage on the same case (17 hand-derived
values — paint order, `#08c` over the 432x152 border box, 32px/700 text at
x=16), so `stage` is a real argument and not a single-stage tool wearing a
parameter; that a diff does **not** disturb the open session, by re-asserting
the session's page at 432 afterwards; and five refusals — unknown stage,
unknown case, unknown reference, a `case` that escapes the case directory, and
a missing argument. Three Rust unit tests pin the path resolver and the name
guard, including that a typo'd path resolves to nothing rather than to a
neighbouring value.

```
$ cargo build -p hiwave-mcp && python3 crates/hiwave-mcp/smoke.py
ok  initialize        {'name': 'hiwave-mcp', 'version': '0.1.0'}
ok  tools/list        ['hiwave_open', 'hiwave_layout', 'hiwave_display_list', 'hiwave_style', 'hiwave_diff', 'hiwave_screenshot', 'hiwave_status']
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
ok  hiwave_diff       hero/layout agrees with the spec reference on 12/12 hand-derived values (incl. border_box 432x152)
ok  hiwave_diff       hero/display_list agrees on 17 values (paint order, #08c over 432x152, 32px/700 text at x=16)
ok  hiwave_diff       important-width DISAGREES: border_box.width expected 100.0 (spec: !important wins), engine computed 400.0 — 2 of 4, height and x still agree
ok  session isolation open page still 432x152 after three diffs
ok  diff guards       unknown stage, unknown case, unknown reference, path escape and missing argument all refused
ok  hiwave_screenshot 1440015 bytes at ppm
ok  argument guard    pass either `html` or `path`, not both

PASS: hiwave-mcp serves the engine's computed layout, its paint commands, the cascade behind them, AND whether any of it agrees with a committed reference
```

**Checked that the gate can go red.** Changed the `hero` case's `padding: 16px`
to `20px` — a change to what the engine computes, with the reference untouched
— and both stages went red with exactly the hand-derived numbers
(400+2·20 = 440, 120+2·20 = 160, x = 20):

```
--- hero/layout: agrees=False differences=5/12
    root.children[0].children[0].border_box.width            expected 432.0  engine 440.0
    root.children[0].children[0].border_box.height           expected 152.0  engine 160.0
    root.children[0].children[0].padding.left                expected 16.0  engine 20.0
    root.children[0].children[0].padding.top                 expected 16.0  engine 20.0
    root.children[0].children[0].children[0].border_box.x    expected 16.0  engine 20.0
--- hero/display_list: agrees=False differences=3/17
    commands[1].rect.width                                   expected 432.0  engine 440.0
    commands[1].rect.height                                  expected 152.0  engine 160.0
    commands[2].x                                            expected 16.0  engine 20.0
```

Perturbation reverted; the committed `page.html` has `padding: 16px`.

### Night 2's open question, answered without needing a ruling

Nights 1 and 2 both asked whether a Linux-verified receipt is acceptable or
whether the trench needs a macOS runner, and both flagged `hiwave_diff` as the
slice where the runner would stop being a workaround. It did not, because the
question was about the wrong `reference`. The plan (§10.3) names two — Chrome,
and a committed macOS capture — and both are captures from elsewhere, so both
would have made this slice runner-bound.

The reference this night ships is neither: it is **hand-derived expectations**,
machine-independent by construction. That is also the only reference kind that
could have adjudicated the `important-width` case at all — no capture of RustKit
can show a bug that RustKit is the source of, and a Chrome screenshot would show
a 100px box without saying which stage lost the declaration.

So the ruling is no longer blocking. It is still needed for the capture-kind
reference below, which is genuinely runner-bound.

### What the engine still cannot answer

- **Capture-kind references are not implemented.** The plan's port-verification
  story (§10.3 — "did my port compute the same thing macOS computes?") needs a
  full committed macOS export diffed structurally against a live one. A
  reference declaring `"kind": "capture"` is today **refused with a message
  saying so** rather than misread. I did not build it because I cannot test it
  here: this runner is Linux, so any capture I produced would be a Linux capture
  diffed against itself — a gate that cannot go red, which is precisely the
  instrument failure this trench exists to avoid. This is the one place a macOS
  runner is load-bearing.
- **`style` is not a diffable stage.** `hiwave_style` answers per-selector, so a
  reference would need a selector per expectation. The stage argument refuses
  `style` rather than approximating it. Tractable, not done.
- **Two cases, not a corpus.** `hero` and `important-width`. Nothing diffs a
  real page, so clipping, stacking contexts, transforms, grid and every form
  control have zero diff coverage — the same gap nights 1 and 2 named for their
  own tools, now inherited here.
- **The `important-width` reference pins a known bug's correct answer, which
  makes it a tripwire.** When the cascade is fixed to honour `!important`, that
  case starts agreeing and three smoke assertions go red. That flip is the
  signal working; it is documented in the case file and in `smoke.py`, but
  whoever fixes the cascade must update the expectation rather than route around
  it.
- Everything nights 1 and 2 named is still true: UA-origin properties have no
  rule to cite, computed values cover 15 longhands, shorthand provenance does
  not reach longhands, `hiwave_style` takes simple selectors only, and the
  display list's unmodelled ops (form controls, carets, focus rings, backdrop
  filters, gradient text, SVG primitives) carry `"modelled": false` with no
  contract and no coverage.

### Tests

`cargo test --workspace --no-fail-fast`, excluding `hiwave-app` and
`hiwave-smoke` (need GTK — `gdk-sys` build fails) and `rustkit-media` (needs
ALSA): **906 passed, 1 failed**. The one failure is
`rustkit-layout::probe_normal_line_height_vs_chrome` — the same pre-existing
failure nights 1 and 2 reported, with the same signature (`0/20 pairs`), and
`rustkit-layout` has no dependency on `hiwave-mcp`, so this night's change
cannot reach it. The three new unit tests ran in that same workspace run and
passed.

Scope stayed inside `crates/hiwave-mcp/`: no parity harness, no `.github/`, no
Windows or Linux port work, and the existing `hiwave_layout` export untouched.
The runner needed `mesa-vulkan-drivers` installed again (fresh container;
environment only, nothing committed — and `apt-get update` first, since the
cached index 404s).

### Decisions needed from Pete

1. **4 of 4 is reached — stop, or open a new loop?** `BASELINE.md`'s stop
   condition is met and I am stopping per the instruction. If a follow-on is
   wanted, the two candidates are the capture-kind reference (needs a macOS
   runner; serves the porting seats) and a real-page case corpus (serves
   diagnosis). Both are new metrics, not continuations of this one.
2. **`!important` is still dead in the cascade** — carried from night 2, now
   pinned by a case so it cannot be quietly forgotten. Still my read that it
   should be filed as a parity-corpus event rather than fixed inside an export
   loop.

---

## 2026-08-02 — night 4 (stop condition — trench complete)

**Metric: 4 of 4 → 4 of 4**

**Moved no → yes: NONE.** No new work was started, and this is not a dry
night: `BASELINE.md`'s stop condition 1 was already met by night 3, and the
instruction on reaching it is to stop rather than to find more to do. Clause 2
(two consecutive dry nights) does not apply — nights 1, 2 and 3 each moved a
tool. So this entry is a close-out, not a funeral.

### What I did instead: re-ran the receipt rather than trusting it

Night 3's receipt was written on the trench branch. Since then it landed on
master (`5aa912d`), and a squash-merge is exactly the kind of step where a
receipt can quietly stop being true. So the one thing worth doing tonight was
checking the four assertions still pass **on the merged tree** — checked out
at `5aa912d`, clean working tree, nothing of mine applied:

```
$ cargo build -p hiwave-mcp && python3 crates/hiwave-mcp/smoke.py
ok  initialize        {'name': 'hiwave-mcp', 'version': '0.1.0'}
ok  tools/list        ['hiwave_open', 'hiwave_layout', 'hiwave_display_list', 'hiwave_style', 'hiwave_diff', 'hiwave_screenshot', 'hiwave_status']
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
ok  hiwave_diff       hero/layout agrees with the spec reference on 12/12 hand-derived values (incl. border_box 432x152)
ok  hiwave_diff       hero/display_list agrees on 17 values (paint order, #08c over 432x152, 32px/700 text at x=16)
ok  hiwave_diff       important-width DISAGREES: border_box.width expected 100.0 (spec: !important wins), engine computed 400.0 — 2 of 4, height and x still agree
ok  session isolation open page still 432x152 after three diffs
ok  diff guards       unknown stage, unknown case, unknown reference, path escape and missing argument all refused
ok  hiwave_screenshot 1440015 bytes at ppm
ok  argument guard    pass either `html` or `path`, not both

PASS: hiwave-mcp serves the engine's computed layout, its paint commands, the cascade behind them, AND whether any of it agrees with a committed reference
```

One assertion per tool, each a hand-derivable value, all four green on master:

- `hiwave_layout` — `.hero border_box = 432.0x152.0` (400+2·16 × 120+2·16)
- `hiwave_display_list` — `#08c` = rgb(0,136,204) filling that same 432×152 rect
- `hiwave_style` — `width=400px` won by `.hero` [0,1,0] over `div` [0,0,1]
- `hiwave_diff` — `important-width` reports expected 100.0 vs engine 400.0

Note the last one is a **red** assertion by design: it pins a known engine bug's
correct answer, so the suite proves the diff can disagree, not only agree.

**Final tally: 4 of 4.** The trench is complete and I am stopping.

### What the engine still cannot answer

Unchanged from night 3 — re-listed rather than waved at, because "complete" here
means the metric hit 4 of 4, not that the exports are finished:

- **Capture-kind references are not implemented** — a reference declaring
  `"kind": "capture"` is refused with a message saying so. This is the one item
  that genuinely needs a macOS runner: producing a capture here would diff a
  Linux capture against itself, a gate that cannot go red.
- **`style` is not a diffable stage** — refused rather than approximated,
  because a reference would need a selector per expectation.
- **Two cases, not a corpus** — `hero` and `important-width`. No real page is
  diffed, so clipping, stacking contexts, transforms, grid and form controls
  have zero coverage across all four tools.
- **UA-origin properties have no rule to cite** — the UA sheet is a hardcoded
  Rust `match`, so `winner: null` cannot distinguish "the UA sheet set it" from
  "nobody set it".
- **Computed values cover 15 longhands**; `line-height`, `font-family`,
  `text-align`, `border-*`, `position` and every background-layer field return
  `null`.
- **Shorthand provenance does not reach longhands** — "which rule set
  `padding-left`" is still not answerable.
- **Unmodelled display-list ops** — form controls, carets, focus rings, backdrop
  filters, gradient text and SVG primitives carry `"modelled": false`, with no
  contract and no coverage.
- **The `important-width` reference is a tripwire**: fixing the cascade to
  honour `!important` will make that case agree and turn three smoke assertions
  red. That flip is the signal working — update the expectation, do not route
  around it.

### Decisions needed from Pete

1. **The trench is closed at 4 of 4 — does a follow-on loop open, and on which
   metric?** Carried unchanged from night 3 because it was never answered, and
   it is the only thing standing between here and the next slice. The two
   candidates remain the capture-kind reference (needs a macOS runner; serves
   the porting seats) and a real-page case corpus (serves diagnosis). Both are
   new metrics with their own baselines, not continuations of this one.
2. **`!important` is dead in the cascade** — carried from nights 2 and 3, still
   unfixed, now pinned by a case. My read is unchanged: file it as a
   parity-corpus event.

No third question. The Linux-vs-macOS runner question that nights 1 and 2 both
raised is not repeated here — night 3 answered it for everything this trench
shipped, and it survives only inside decision 1, where it is a property of the
capture-kind option rather than an open ruling.

---

## 2026-08-03 — night 5 (no-op firing; the trench is already closed)

**Metric: 4 of 4 → 4 of 4. Moved no → yes: NONE.**

No work started, and no re-verification run either. `BASELINE.md`'s stop
condition 1 was met by night 3 and the trench was closed out by night 4
(`cf92bfa`), which re-ran the full smoke suite on the merged master tip
`5aa912d`. `origin/master` is **still `5aa912d`** and this branch is still
`cf92bfa`, so nothing has changed underneath that receipt — re-running it
tonight would produce the same output for the same tree, which is motion, not
evidence. Night 4's paste stands as the receipt for all four tools.

**The only new fact tonight is about the loop, not the metric:** the nightly
schedule is still firing after the trench reached its stop condition, and the
close-out commit `cf92bfa` is pushed but unmerged with no PR open (PR #79
carried nights 1–3 and is closed). Both need Pete or Atlas, not another night.

**A future firing should not append another entry.** If the schedule is still
enabled and the state is unchanged, the correct action is to notify and stop —
one close-out is the record; a nightly chorus of them is padding.

### What the engine still cannot answer

Unchanged from night 4 and not re-listed here: capture-kind references, `style`
as a diffable stage, a real-page case corpus, UA-origin rule citation, the 15
covered longhands, shorthand→longhand provenance, the unmodelled display-list
ops, and the `important-width` tripwire. See night 4's entry for each in full.

### Decisions needed from Pete

1. **Turn the nightly schedule off, or point it at a new metric?** It is firing
   nightly against a completed trench. This is the same question night 3 and
   night 4 asked about a follow-on loop, now with a cost attached.
2. **`cf92bfa` (the close-out entry) is unmerged and has no PR** — Atlas opens
   PRs by design, so this is a handoff note, not a request to open one.

`!important` in the cascade is deliberately not repeated as a third question:
nights 2, 3 and 4 all filed it with the same read (a parity-corpus event, not a
trench fix), and repeating it a fourth time would be manufacturing volume.

---
---

# TRENCH 2 — computed-style answer coverage

New metric, pinned in `BASELINE.md` on Pete's ruling of 2026-08-03: *"Point at
new metrics, eat off the next chunk of the elephant."* Trench 1 closed at 4 of
4; this one counts **properties `hiwave_style` can answer with a value the
engine COMPUTED and a provenance that does not lie**, over a named 12-property
diagnosis set. Same branch, same receipt discipline, same scope limits.

---

## 2026-08-03 — night 6 (`text-align`, `font-family`)

**Metric: 0 of 12 → 2 of 12**

**Moved no → yes: `text-align` and `font-family`.** `line-height` is
implemented, asserted, and deliberately **NOT counted** — see the divergence
below. That is the whole story of the night.

### The assertions that prove it

The fixture gained one rule and one subtree, both load-bearing:

```css
.copy { font-size: 20px; line-height: 1.5; font-family: Georgia, serif; text-align: center }
```
```html
<div class="copy"><span>inherited</span></div>
```

The `span` declares **nothing**. So every value it reports came down the tree,
and the assertion is not "a value came back" — it is that the engine says
*where from*:

```python
assert not [d for d in span["declared"] if d["winner"] is not None], span["declared"]
for prop, expected in (("font-family", "Georgia, serif"),
                       ("text-align", "center")):
    d = next(x for x in span["declared"] if x["property"] == prop)
    assert d["computed"] == expected, d
    assert d["winner"] is None, d
    assert d["origin"] == "inherited", d
# The distinction is only worth anything if it can still say UA/initial:
# `display` does not inherit, so the span's block/inline default is NOT
# inheritance even though .copy also has a value for it.
disp = next(x for x in span["declared"] if x["property"] == "display")
assert disp["origin"] == "user-agent-or-initial", disp
```

Before tonight, every one of those came back `"origin": "user-agent-or-initial"`
— "nothing declared this" was one answer where CSS has three, and the one it
collapsed is the only one that points at **another element**. An agent chasing a
wrong font on the span was told the property was unset; it is now sent to
`.copy`.

`line-height` is also implemented, and its value assertion is real and
hand-derived — 20px x 1.5 = 30px, where no rule in the fixture spells a px
line-height, so an echoing serializer would report `1.5`:

```python
assert copy["computed"]["line-height"] == "30px", copy["computed"]
lh = next(d for d in copy["declared"] if d["property"] == "line-height")
assert lh["winner"]["value"] == "1.5", lh    # authored as a bare multiplier
assert lh["computed"] == "30px", lh          # ...reported as pixels
```

`normal` is deliberately left as the keyword rather than resolved to px: it
derives from the font's own ascent/descent/line-gap, so a number there would
look machine-independent and would not be.

```
$ cargo build -p hiwave-mcp && python3 crates/hiwave-mcp/smoke.py
ok  initialize        {'name': 'hiwave-mcp', 'version': '0.1.0'}
ok  tools/list        ['hiwave_open', 'hiwave_layout', 'hiwave_display_list', 'hiwave_style', 'hiwave_diff', 'hiwave_screenshot', 'hiwave_status']
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
ok  line-height       .copy 20px x 1.5 = 30px — authored '1.5', computed 30px (resolved, not echoed)
ok  normal not faked  .hero line-height=normal (keyword, not px — resolving it needs font metrics)
ok  inherited origin  span font-family='Georgia, serif' text-align=center both origin=inherited; display still UA-or-initial
ok  KNOWN DIVERGENCE  span line-height reported 'normal' but laid out at 30px — inherited in build_layout_box, after the trace
ok  selector guard    refused 'div p'
ok  hiwave_diff       hero/layout agrees with the spec reference on 12/12 hand-derived values (incl. border_box 432x152)
ok  hiwave_diff       hero/display_list agrees on 17 values (paint order, #08c over 432x152, 32px/700 text at x=16)
ok  hiwave_diff       important-width DISAGREES: border_box.width expected 100.0 (spec: !important wins), engine computed 400.0 — 2 of 4, height and x still agree
ok  session isolation open page still 432x152 after three diffs
ok  diff guards       unknown stage, unknown case, unknown reference, path escape and missing argument all refused
ok  hiwave_screenshot 1440015 bytes at ppm
ok  argument guard    pass either `html` or `path`, not both

PASS: hiwave-mcp serves the engine's computed layout, its paint commands, the cascade behind them, AND whether any of it agrees with a committed reference
```

**Checked that the gate can go red.** Changed the fixture's `line-height: 1.5`
to `2` and the tool reported `40px` — exactly what hand-derivation predicts from
20 x 2 — failing the 30px assertion:

```
AssertionError: {... 'font-size': '20px', 'line-height': '40px', ...}
```

The number tracks the engine, not the call. Perturbation reverted; the
committed fixture says `1.5`.

### The finding: `hiwave_style`'s line-height is not layout's line-height

This is why `line-height` is not counted, and it is the more valuable half of
the night. The cascade seeds inherited properties from the parent in
`compute_style_for_element` — font-size, font-family, font-weight, font-style,
font-stretch, color, letter-spacing, word-spacing, text-align — and
**deliberately not line-height**, whose own comment says so. Line-height is
inherited one layer later, in `build_layout_box`:

```rust
// The UA defaults in `compute_style_for_element` never set `line_height`, so a value
// of `Normal` here reliably means "not specified by UA or author": inherit the
// parent's computed value.
if let Some(parent) = parent_style {
    if matches!(style.line_height, rustkit_css::LineHeight::Normal)
        && !matches!(parent.line_height, rustkit_css::LineHeight::Normal)
    { style.line_height = parent.line_height.clone(); }
}
```

The style trace is recorded during the cascade, so it sees the **pre**-
inheritance value. For the span: `hiwave_style` reports `normal`; layout lays it
out at 30px. Per `BASELINE.md` clause 3, a property whose reported value can
differ from the value layout used does not count — a tool that disagrees with
the engine is worse than a gap.

I did **not** fix it. Moving that inheritance into the cascade is plausibly
value-identical (the later guard would then find the value already set and do
nothing), but "plausibly value-identical" on the inherited line-height of every
element is a parity-corpus event, not an export slice — and this trench's own
scope limit now says so in writing. Instead it is pinned as a tripwire, the way
night 3 pinned `!important`:

```python
span_lh = next(x for x in span["declared"] if x["property"] == "line-height")
assert span_lh["computed"] == "normal", span_lh
assert span_lh["origin"] == "user-agent-or-initial", span_lh
```

Whoever moves that inheritance will see this go red, and should then assert
`"30px"` and count the property — not route around it.

### What the engine still cannot answer

- **`line-height` for any element that inherits it** — the divergence above. The
  value is right only where the element declares it directly.
- **Ten of the twelve**: `font-style`, `letter-spacing`, `white-space`,
  `border-top-width`, `border-top-color`, `box-sizing`, `position`,
  `overflow-x`, `opacity` are not in the computed set at all, and `line-height`
  is uncounted. The border pair is the one that forces shorthand→longhand
  provenance, still unbuilt.
- **Non-px lengths come back as Rust `Debug` strings, not CSS.** The perturbation
  output above shows it plainly: `'height': 'Auto'`, `'margin-top': 'Zero'`,
  `'padding-left': 'Zero'`. An agent comparing against Chrome's `auto` / `0px`
  gets a spurious mismatch. It is a small fix in `computed_value_of`'s `len()`
  helper and it is the cheapest correct thing left; I did not fold it into
  tonight because it changes values the existing `hiwave_diff` references may
  quote, so it wants its own slice and its own red-check.
- **`inherited` is decided by value equality plus CSS inheritance, guarded by a
  small explicit shorthand table** (`font` → font-size/family/weight and
  line-height). A shorthand outside that table which sets an inherited property
  to exactly the parent's value would still be mislabelled `inherited`. Two unit
  tests pin both halves and the shorthand guard; the general expansion model is
  the same missing piece trench 1 named.
- Everything trench 1 named is still true: UA-origin properties have no rule to
  cite, `hiwave_style` takes simple selectors only, capture-kind references are
  refused, `style` is not a diffable stage, and there is no real-page corpus.

### Tests

`cargo test -p rustkit-engine`: **31 passed, 0 failed** — including the two new
unit tests and `test_line_height_inherits_from_html_through_body`, which is the
test that documents the layout-side inheritance path described above.
`cargo test -p hiwave-mcp`: **3 passed, 0 failed**.

I ran the two crates I touched rather than the whole workspace, because the
night's cap was up and the change is additive recording plus serialization — it
reads `ComputedStyle` and writes JSON, and applies no property, so no rendering
path changes. That is a smaller run than nights 1–3 did and I am flagging it as
such rather than implying a workspace run happened. The runner needed
`mesa-vulkan-drivers` installed again (fresh container; environment only,
nothing committed).

### Decisions needed from Pete

1. **The line-height inheritance split — move it into the cascade, or leave the
   tripwire?** Moving it would make the tool and layout agree and would count
   the property; it also touches the inherited line-height of every element, so
   it is a parity-corpus event. My read: leave it, and let whoever next works
   the text bucket in the parity trench do it there, where the corpus is
   watching.

Next slice: `font-style`, `letter-spacing` and `white-space` — the rest of the
text group, all three seeded by the cascade already (so no divergence expected),
each needing a fixture case where the value is inherited or converted rather
than echoed.
