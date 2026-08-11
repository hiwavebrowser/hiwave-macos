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

---

## 2026-08-04 — night 7 (`font-style`, `letter-spacing`; `white-space` is a finding)

**Metric: 2 of 12 → 4 of 12**

**Moved no → yes: `font-style` and `letter-spacing`.** `white-space` is
implemented and asserted and deliberately **NOT counted** — see the finding
below. That is the rest of the text group, and the divergence is the more
useful half of the night.

### First, the stored prompt is still stale

Tonight's firing again described the metric as trench 1's "four Tier-1 MCP
reads" and told me to stop at 4 of 4. `BASELINE.md` (which the same prompt
names as binding, and reads first) says that trench closed on 2026-08-02 and
that trench 2 is the live metric. I worked trench 2 and report `N of 12`, per
the file. Night 6 already flagged this; it is now the second night spent on a
prompt that names a finished trench, so it is a decision below rather than a
note.

### The assertions that prove it

The fixture gained one declaration and three subtrees.

**`font-style`** is asserted on a span that declares **nothing**, so `italic`
came down the tree from `.copy` and cannot be an echo of declaration text:

```python
fstyle = next(x for x in span["declared"] if x["property"] == "font-style")
assert fstyle["computed"] == "italic", fstyle
assert fstyle["winner"] is None, fstyle          # nothing on the span said so
assert fstyle["origin"] == "inherited", fstyle   # ...`.copy` did
italic_run = [c for c in commands if c["op"] == "text" and c["text"] == "inherited"]
assert italic_run[0]["font_style"] == 1, italic_run[0]   # 0 normal, 1 italic
# ...and the h1, which inherits nothing italic, is still upright — so the
# flag tracks the cascade rather than being on for every run.
assert glyphs["font_style"] == 0, glyphs
```

The second half is what makes it *count* rather than merely pass: the paint
command carries the italic flag, so the value the tool reports is the value the
shaper picked a face with. Clause 3 asks whether the tool can disagree with the
engine; this checks the two against each other instead of asserting the cascade
twice.

**`letter-spacing`** is a unit conversion — `0.1em` on a 20px element is 2px,
and no rule in the fixture spells a px letter-spacing:

```python
assert spaced["computed"]["letter-spacing"] == "2px", spaced["computed"]
ls = next(d for d in spaced["declared"] if d["property"] == "letter-spacing")
assert ls["winner"]["value"] == "0.1em", ls      # authored as a multiple of em
assert ls["computed"] == "2px", ls               # ...reported as pixels
```

and then the half that makes it count — that 2px is the number **layout**
spaced glyphs by. `.plain` and `.spaced` carry the same text, family and size
and differ in exactly one declaration, so every font metric cancels in the
difference and what is left is the spacing alone:

```python
runs = [c for c in commands if c["op"] == "text" and c["text"] == "spacing"]
plain_run, spaced_run = sorted(runs, key=lambda c: c["index"])
assert len(plain_run["advances"]) == len(spaced_run["advances"]) == len("spacing") == 7
deltas = [round(s - p, 4) for p, s in zip(plain_run["advances"], spaced_run["advances"])]
assert deltas == [2.0] * 7, deltas
```

A **delta**, not an absolute advance, and deliberately: the absolute values are
this runner's font, the delta is the engine's arithmetic. (Worth saying plainly:
this runner has no Georgia, so its fallback advances happen to be uniform, and
on this machine the absolutes would have passed too. The delta form is what
makes the assertion portable to a runner where they are not — it is written for
the machine it may move to, not the one it ran on.)

The resolution path is the same one layout takes — px as-is, em against the
element's own computed font-size, rem against 16 — so the reported number
cannot drift from the one the shaper used. Arms layout *cannot* resolve return
a hole instead of layout's silent `0.0` fallback, because `0px` for a
percentage is a confident wrong answer where `null` is a true one.

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
ok  font-style        span computed=italic origin=inherited (declares nothing); paint drew it font_style=1, h1 still 0
ok  letter-spacing    .spaced 0.1em x 20px = 2px (authored '0.1em'); every advance is exactly +2.0 over .plain — [2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0]
ok  KNOWN DIVERGENCE  .pre keeps 'a  b' (4 advances) but its nested span collapsed 'c  d' to 'c d' — white-space is not inherited onto elements
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

**Checked that the gate can go red — three times, once per claim.**

1. `letter-spacing: 0.1em` → `0.2em`: reported `4px`, exactly what hand
   derivation predicts from 0.2 x 20, failing the 2px assertion.
   ```
   AssertionError: {... 'font-size': '20px', 'letter-spacing': '4px', ...}
   ```
2. `.copy`'s `font-style: italic` → `normal`: the span reports `normal`, so the
   value tracks the parent's declaration rather than being a constant.
   ```
   AssertionError: {'computed': 'normal', 'origin': 'inherited', 'property': 'font-style', 'winner': None}
   ```
3. The white-space tripwire, which is the one that matters, because a tripwire's
   whole claim is that it fires when the bug is fixed. Adding the single line
   `style.white_space = parent.white_space` to `compute_style_for_element` —
   i.e. actually fixing the inheritance — makes the nested span report `pre` and
   the tripwire goes red:
   ```
   AssertionError: {'computed': 'pre', 'origin': 'user-agent-or-initial', 'property': 'white-space', 'winner': None}
   ```

All three perturbations reverted; `grep -r RED-CHECK crates/` returns 0 on the
committed tree.

### The finding: `white-space` does not inherit onto elements, and text collapses

`white-space` inherits in CSS. This cascade never seeds it onto an **element** —
`compute_style_for_element` seeds font-size/family/weight/style/stretch/colour/
letter-spacing/word-spacing/text-align from the parent, and white-space is not
in that list. It is inherited one layer down, in `build_layout_box`, and only
onto **text nodes**, from their immediate parent:

```rust
// Wrapping behavior is inherited; without these a
// nowrap/pre parent's text still wrapped (shelf bug).
s.white_space = parent.white_space;
```

So a text node directly inside `<div class="pre">` gets `Pre`, but a text node
inside a `<span>` inside that div gets the span's own initial `Normal` — the
span was never told. **The consequence is real and visible in paint**, which is
what makes this a bug report rather than a reporting quirk:

```
{"index": 6, "text": "a  b", "n_adv": 4}   <- the div's own text: both spaces kept
{"index": 7, "text": "c d",  "n_adv": 3}   <- the nested span's: collapsed
```

Same `pre` ancestor, one element apart, different answers. Chrome keeps both.

`white-space` is therefore **not counted**, on two independent grounds, and it
is worth separating them:

- On the element that *declares* it, the reported value is an **echo** of the
  declaration text — clause 2 fails. The natural non-echo route for this
  property is inheritance, and inheritance is exactly what is broken.
- Its provenance would otherwise lie. `inherited_properties` decides the
  `inherited` label by value equality plus CSS inheritance, and by that rule a
  child matching its parent's `normal` would be labelled `inherited` when the
  cascade never seeded it — pointing an agent at a parent that supplied
  nothing. `white-space` is now **explicitly excluded** from that label, with
  the reason in the code.

I did **not** fix it, per this trench's scope limit, which now says so in
writing: the one-line seeding above is a change to the inherited white-space of
every element and therefore a parity-corpus event, not an export slice. The
tripwire and the `limits` payload both carry the reason, and the smoke comment
names the second half a fixer must not miss — dropping the exclusion in
`inherited_properties`, or the origin will keep saying `user-agent-or-initial`
for a value that is by then genuinely inherited.

This is the **second** property in the diagnosis set (after `line-height`) whose
inheritance happens below the cascade. Two of twelve is a pattern, not a
coincidence, and it is the same shape both times: the cascade is not the single
source of computed style. Named here rather than absorbed.

### What the engine still cannot answer

- **`white-space` for any element that inherits it**, and **`line-height` for
  any element that inherits it** (night 6). Both values are right only where the
  element declares them directly.
- **Eight of the twelve remain**: `border-top-width`, `border-top-color`,
  `box-sizing`, `position`, `overflow-x`, `opacity` are not in the computed set
  at all, and `line-height` and `white-space` are implemented-but-uncounted. The
  border pair is next in `BASELINE.md`'s order and is the one that forces
  shorthand→longhand provenance, still unbuilt.
- **Non-px lengths still come back as Rust `Debug` strings** — `'height':
  'Auto'`, `'margin-top': 'Zero'`, `'padding-left': 'Zero'` are all visible in
  tonight's own probe output. Named by night 6 as the cheapest correct thing
  left, and still not done: it changes values the existing `hiwave_diff`
  references quote, so it wants its own slice and its own red-check. Tonight's
  new arms avoid adding to the pile — `letter-spacing: 0` reports `0px`, not
  `Zero` — which makes the inconsistency *within one payload* worse, not
  better, until that slice lands. Flagging that honestly rather than counting it
  as tidy.
- **`letter-spacing` in percent or any other unresolvable unit returns `null`**
  rather than layout's silent `0.0`. Honest, but it is a hole: an agent asking
  why a percentage letter-spacing did nothing is told nothing rather than told
  that layout dropped it.
- Everything trench 1 named is still true: UA-origin properties have no rule to
  cite, `hiwave_style` takes simple selectors only, shorthand provenance does
  not reach longhands, capture-kind references are refused, `style` is not a
  diffable stage, and there is no real-page corpus.

### Tests

`cargo test --workspace --no-fail-fast`, excluding `hiwave-app` and
`hiwave-smoke` (need GTK — `gdk-sys` build fails) and `rustkit-media` (needs
ALSA): **882 passed, 1 failed**. The one failure is
`rustkit-layout::probe_normal_line_height_vs_chrome`, the same pre-existing
failure nights 1–3 reported, at `normal_line_height_probe.rs:87`. Confirmed it
cannot be mine structurally rather than by assertion: `rustkit-layout`'s
dependencies are `rustkit-dom`, `rustkit-css` and `rustkit-text` — it does not
depend on `rustkit-engine`, which is the only crate this night changed outside
the smoke script.

`cargo test -p rustkit-engine`: **33 passed, 0 failed**, including the two new
unit tests (one pinning that letter-spacing resolves the way layout does *or
returns a hole*, one pinning that white-space is never labelled `inherited` and
that its CSS spelling is kebab-cased — `{:?}`.to_lowercase() would emit
`prewrap` and `breakspaces`, which are not CSS values).

Scope stayed inside `crates/rustkit-engine/src/lib.rs` and
`crates/hiwave-mcp/smoke.py`: no parity harness, no `.github/`, no Windows or
Linux port work, no engine behaviour changed (the new code reads `ComputedStyle`
and writes JSON; it applies no property), and the exports that already had
passing assertions were left alone — the one edit to an existing assertion is
the `span` → `#inherits` selector, forced because the fixture now has four
spans and a bare `span` query would silently average four elements into one
answer. The runner needed `mesa-vulkan-drivers` installed again (fresh
container; environment only, nothing committed).

### Decisions needed from Pete

1. **The nightly prompt still describes trench 1 and tells the agent to stop at
   4 of 4.** Two nights have now had to override it from `BASELINE.md`. It
   cannot be edited from inside a session (created via the API; agents may only
   edit routines they created), so it needs you. Until then every night spends
   its first move deciding whether to believe the prompt or the repo — and the
   failure mode is silent: a night that believes the prompt stops immediately
   and reports a completed trench.
2. **Two of twelve properties inherit below the cascade** (`line-height`,
   `white-space`), both now pinned as tripwires rather than fixed. If the answer
   to "move them into the cascade" is yes, it is one change with one parity
   run, not two — worth doing together, and worth doing in the parity trench
   where the corpus is watching. My read is unchanged from night 6: leave them,
   but the count is now two and the next one will make it three.

Next slice: `border-top-width` and `border-top-color` — the shorthand group, and
the one that forces shorthand→longhand provenance, which trench 1 named as the
single place the output can currently mislead (`padding-left` reports
`"winner": null` when `padding: 16px` plainly set it). Expect that to be the
whole slice: the values are easy, the provenance is the work.

---

## 2026-08-05 — night 8 (`border-top-width`, `border-top-color`)

**Metric: 4 of 12 → 6 of 12**

**Moved no → yes: `border-top-width` and `border-top-color`.** That is the
whole shorthand group, and the mechanism behind it is the more interesting
half: attribution is now *measured* rather than predicted.

### The stored prompt is still stale (third night)

Tonight's firing again described the metric as trench 1's "four Tier-1 MCP
reads" and told me to stop at 4 of 4. `BASELINE.md` — which the same prompt
names as binding, and which I read first — says trench 1 closed on 2026-08-02
and trench 2 is live. I worked trench 2 and report `N of 12`. Nights 6 and 7
flagged this; it is now three nights, and it remains a decision below. A worse
wrinkle showed up this time: the scheduler's checkout was on **master**, whose
`trench/digest.md` stops at night 3 and whose `BASELINE.md` has no trench 2 in
it at all. A night that read those files without also fetching this branch
would have seen "4 of 4, stop condition met" in both the prompt *and* the repo,
and stopped. The branch is the only place the live metric exists.

### The assertions that prove it

Three fixture rules, each load-bearing for a different claim:

```css
.framed    { width: 200px; height: 40px; font-size: 20px; border: 0.25em solid #c60 }
.hairline  { width: 200px; height: 40px; border: 2px solid }
.overruled { width: 200px; height: 40px; border: 3px solid #093; border-top-width: 9px }
```

**`border-top-width`** is two conversions away from the declaration text:
nothing in the fixture spells `border-top-width`, and nothing spells a px
border width. 5px requires the engine to expand the shorthand *and* resolve
the em against the computed font-size.

```python
assert framed["computed"]["border-top-width"] == "5px", framed["computed"]
btw = next(d for d in framed["declared"] if d["property"] == "border-top-width")
assert btw["winner"]["property"] == "border", btw["winner"]
assert btw["winner"]["via_shorthand"] is True, btw["winner"]
assert btw["winner"]["value"] == "0.25em solid #c60", btw["winner"]
assert btw["winner"]["selector"] == ".framed", btw["winner"]
```

**`border-top-color`** is a hex-triplet expansion — `#c60` is rgb(204, 102, 0),
0xcc = 204 and 0x66 = 102 by hand — cited to the same shorthand.

And the half that makes both **count** rather than merely pass: 5px is the
width layout reserved and the height paint drew, in that colour.

```python
framed_box = find(tree["root"],
                  lambda n: (n.get("border_box") or {}).get("width") == 210.0)
assert framed_box["border"]["top"] == 5.0, framed_box["border"]   # 200 + 2*5
assert framed_box["content_rect"]["width"] == 200.0, framed_box["content_rect"]
bands = [c for c in commands
         if c["op"] == "solid_color"
         and c["color"] == {"r": 204, "g": 102, "b": 0, "a": 1.0}
         and c["rect"]["height"] == 5.0]
assert len(bands) == 2, [c["rect"] for c in bands]     # top and bottom
top_band = [c for c in bands if c["rect"]["y"] == framed_box["border_box"]["y"]]
assert top_band[0]["rect"]["width"] == 210.0, top_band[0]["rect"]
```

**The assertion the whole mechanism exists for.** `border: 2px solid` carries
no colour, so `parse_border_shorthand` returns `None` for it and the cascade
leaves `border_top_color` alone. A hand-written expansion table — "`border`
sets the four widths and the four colours" — would cite that rule as the
source of a colour it never wrote: a confident, plausible lie of exactly the
kind clause 3 exists to exclude.

```python
hw = next(d for d in hair["declared"] if d["property"] == "border-top-width")
assert hw["winner"]["property"] == "border", hw["winner"]      # the width: cited
hc = next(d for d in hair["declared"] if d["property"] == "border-top-color")
assert hc["winner"] is None, hc                                # the colour: NOT
assert hc["origin"] == "user-agent-or-initial", hc
```

Plus the ordering a merged property list has to get right — a longhand after a
shorthand wins, and the shorthand is reported as beaten rather than dropped:

```python
assert ow["computed"] == "9px", ow
assert ow["winner"]["property"] == "border-top-width", ow["winner"]
assert ow["winner"]["via_shorthand"] is False, ow["winner"]
assert ow["overridden"][0]["property"] == "border", ow["overridden"]
assert over_box["border"] == {"top": 9.0, "right": 3.0, "bottom": 3.0, "left": 3.0}
assert over_box["border_box"]["height"] == 52.0, over_box["border_box"]  # 40 + 9 + 3
```

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
ok  computed expansion padding-left=16px, cited to `padding: 16px` on .hero (via_shorthand) — no rule spells the longhand
ok  origin split      h1 font-weight=700 winner=None (UA, no rule to cite); font-size=32px winner=h1 (author)
ok  line-height       .copy 20px x 1.5 = 30px — authored '1.5', computed 30px (resolved, not echoed)
ok  normal not faked  .hero line-height=normal (keyword, not px — resolving it needs font metrics)
ok  inherited origin  span font-family='Georgia, serif' text-align=center both origin=inherited; display still UA-or-initial
ok  KNOWN DIVERGENCE  span line-height reported 'normal' but laid out at 30px — inherited in build_layout_box, after the trace
ok  font-style        span computed=italic origin=inherited (declares nothing); paint drew it font_style=1, h1 still 0
ok  letter-spacing    .spaced 0.1em x 20px = 2px (authored '0.1em'); every advance is exactly +2.0 over .plain — [2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0]
ok  KNOWN DIVERGENCE  .pre keeps 'a  b' (4 advances) but its nested span collapsed 'c  d' to 'c d' — white-space is not inherited onto elements
ok  border shorthand  .framed border-top-width=5px (0.25em x 20px) and border-top-color=rgba(204, 102, 0, 1), both cited to `border` on .framed; layout reserved 5.0 and paint drew a 210.0x5.0 band
ok  no false citation `border: 2px solid` cites the width and NOT border-top-color (the declaration carried no colour)
ok  longhand wins     .overruled border-top-width=9px beats `border: 3px solid #093`, which is reported in overridden; layout reserved {'bottom': 3.0, 'left': 3.0, 'right': 3.0, 'top': 9.0} and a 52.0-tall box
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

**Checked that the gate can go red — twice, once per claim.**

1. `.framed`'s `border: 0.25em` → `0.4em`: reported `8px`, exactly 0.4 x 20 by
   hand, failing the 5px assertion.
   ```
   AssertionError: {... 'border-top-color': 'rgba(204, 102, 0, 1)', 'border-top-width': '8px', 'font-size': '20px', ...}
   ```
2. The one that matters. Replaced the measurement with a name-based table —
   `if property == "border" { return vec!["border-top-width", "border-top-color"] }`
   — i.e. exactly the implementation a reasonable person would write first. The
   `.hairline` assertion catches it citing a rule for a colour that rule never
   set:
   ```
   AssertionError: {'computed': 'rgba(0, 0, 0, 1)', 'origin': 'author', 'overridden': [],
    'property': 'border-top-color',
    'winner': {'property': 'border', 'selector': '.hairline', 'specificity': [0, 1, 0],
               'value': '2px solid', 'via_shorthand': True, ...}}
   ```
   Note what the perturbed tool reports: black, sourced to `.hairline`. Both
   halves plausible, the citation false.

Both perturbations reverted; `grep -r RED-CHECK crates/` returns nothing on the
committed tree.

### How the attribution works, and why not a table

`longhands_written` (rustkit-engine) answers "did this declaration write this
longhand" by **running the applying function**: two copies of the style as it
stood before the declaration are made to disagree in exactly the longhand under
test, the declaration is applied to both, and if they come out agreeing it wrote
it. The two copies must be tellable apart *before* the declaration runs, or a
property that reads as a hole on both would look written; that guard is pinned
by a unit test over the whole recorded set.

This is the same principle as night 2's: provenance is recorded from inside the
cascade rather than by a second pass, because a second opinion can drift from
the engine. A shorthand expansion table is exactly such a second opinion, and
`border: 2px solid` is the case where it is wrong.

### One existing assertion changed, disclosed rather than smuggled

`padding-left` reported `"winner": null` from night 2 until tonight. Nights 2,
3 and 7 all named it the one place this output could mislead — literally true
(no rule spells the longhand) and read by anyone as "nothing set this", when
`padding: 16px` plainly did. The general mechanism fixes it, so the night-2
assertion that pinned the null now pins the citation instead:

```python
assert pad_left["winner"]["property"] == "padding", pad_left["winner"]
assert pad_left["winner"]["via_shorthand"] is True, pad_left["winner"]
assert pad_left["winner"]["selector"] == ".hero", pad_left["winner"]
```

This is the one edit to an already-passing export this night makes, and it is
a supersession rather than a tidy-up: the old assertion pinned an answer the
trench had already called misleading. The computed half is untouched — the
value is still the engine's expansion, and the winner's own value is the
shorthand's `16px`, not a longhand nobody wrote.

### What the engine still cannot answer

- **Six of twelve remain.** `box-sizing`, `position`, `overflow-x` and
  `opacity` are not in the computed set at all — the box group, `BASELINE.md`'s
  last block and the cheapest of the three. `line-height` (night 6) and
  `white-space` (night 7) stay implemented-but-uncounted, both because they
  inherit *below* the cascade; nothing about tonight changes that.
- **Only the TOP border side is exported.** `border-right-width`,
  `border-bottom-color` and the rest are not in the recorded set. The diagnosis
  set names the top pair and the mechanism is per-side-agnostic, but an agent
  asking about the right border gets nothing. Adding them is a table entry each
  and no new mechanism — deliberately left, so this night's claim is exactly
  what was asserted.
- **`border-style` is invisible.** The cascade parses it only to decide whether
  the width collapses to zero (`none`/`hidden`); it is stored nowhere, so
  `hiwave_style` cannot report it and cannot distinguish `border: 0 solid` from
  `border: 3px none`. Paint has the same blindness — every border side paints
  as a solid band regardless of the declared style, so `dashed` and `dotted`
  render as solid. Named here rather than absorbed; it is an engine gap, not a
  reporting one, and no smoke assertion covers it.
- **Percent and viewport border widths return null.** Layout resolves them
  against the containing block, which the style trace does not keep. A hole
  rather than a guess, and the same choice `letter-spacing` made — but still a
  hole: an agent asking why a `border-top-width: 10%` did what it did is told
  nothing.
- **Non-px lengths still come back as Rust `Debug` strings.** Visible in
  tonight's own red-check output: `'margin-top': 'Zero'`, `'padding-left':
  'Zero'`. Named by nights 6 and 7 as the cheapest correct thing left, still
  not done, and tonight makes the inconsistency *within one payload* worse
  rather than better — `border-top-width` reports `0px` where `padding-left`
  reports `Zero`. It changes values the existing `hiwave_diff` references
  quote, so it wants its own slice and its own red-check.
- **The `!important` finding is untouched** and still pinned by the
  `important-width` diff case, as are trench 1's limits: UA-origin properties
  have no rule to cite, `hiwave_style` takes simple selectors only,
  capture-kind references are refused, `style` is not a diffable stage, and
  there is no real-page corpus.

### Tests

`cargo test --workspace --no-fail-fast`, excluding `hiwave-app` and
`hiwave-smoke` (need GTK — `gdk-sys` build fails) and `rustkit-media` (needs
ALSA): **913 passed, 1 failed**. The one failure is
`rustkit-layout::probe_normal_line_height_vs_chrome` — the same pre-existing
failure nights 1-3 and 7 reported, same signature (`rounded model matched
Chrome exactly on only 0/20 pairs`, `normal_line_height_probe.rs:87`). It
cannot be tonight's, structurally: `rustkit-layout`'s dependencies are
`rustkit-dom`, `rustkit-css` and `rustkit-text` — it does not depend on
`rustkit-engine`, the only crate changed outside the smoke script. The two new
unit tests ran in that workspace run and passed.

`cargo build --release -p parity-capture` — the only thing CI actually builds
— finishes clean, so nothing here slips the workspace gate.

Scope stayed inside `crates/rustkit-engine/src/lib.rs` and
`crates/hiwave-mcp/smoke.py`: no parity harness, no `.github/`, no Windows or
Linux port work, and **no engine behaviour changed** — `longhands_written`
applies declarations only to throwaway copies, and the applying path itself is
byte-identical when recording is off. The runner needed `mesa-vulkan-drivers`
installed again (fresh container; environment only, nothing committed).

### Decisions needed from Pete

1. **The nightly prompt still describes trench 1**, and tonight it was worse
   than a nuisance: the scheduler's working tree was on `master`, where neither
   `trench/digest.md` nor `BASELINE.md` knows trench 2 exists. Prompt and repo
   agreed on the wrong answer, and only fetching this branch disagreed. It
   cannot be edited from inside a session (created via the API; agents may only
   edit routines they created). Two ways out, both yours: repoint the prompt, or
   land this branch on master so the checkout tells the truth on its own.
2. **Nothing else.** `!important` and the two below-the-cascade inheritances
   were filed in nights 2, 3, 4, 6 and 7 with the same read each time; repeating
   them a sixth time would be manufacturing volume, and none of them blocks the
   remaining slice.

Next slice: the box group — `box-sizing`, `position`, `overflow-x`, `opacity`.
Cheapest of the three groups and the last block in `BASELINE.md`'s order.
`box-sizing` is the one worth care: it silently redefines what `width` means,
so the assertion should be a border box that differs from the declared width,
cross-checked against layout rather than read back off the cascade.

---

## 2026-08-06 — night 9 (`box-sizing`, `position`)

**Metric: 6 of 12 → 8 of 12**

**Moved no → yes: `box-sizing` and `position`.** That is the first half of the
box group. The other half — `overflow-x` and `opacity` — is **reported but not
counted**, and the reason is a finding rather than a shortfall in the export.

### The stored prompt is still stale (fourth night), and tonight it nearly won

The firing again described the metric as trench 1's "four Tier-1 MCP reads" and
told me to stop at 4 of 4. Worse, exactly as night 8 predicted, the scheduler's
checkout was on **master**, whose `trench/digest.md` ends at night 3 and whose
`BASELINE.md` has no trench 2 in it. Prompt and repo agreed on "4 of 4, stop
condition met"; only fetching this branch disagreed. Night 8's warning inside
`BASELINE.md` is what stopped me from closing the loop on a stale metric — it
worked, but it is a tripwire, not a fix, and it only works if the next night
reads the branch's copy rather than master's. Decision below, unchanged.

### The assertions that prove it

Both properties are **keywords**, so no unit conversion or shorthand expansion
can show the value was computed rather than echoed. Two other things do it.

First, the fixture's last rule matches the same elements and loses:

```css
.bordered  { width: 200px; height: 40px; padding: 10px; border: 5px solid #333;
             box-sizing: border-box }
.content   { width: 200px; height: 40px; padding: 10px; border: 5px solid #333 }
.host      { width: 300px }
.floater   { position: absolute; width: 50px; height: 40px }
div { box-sizing: content-box; position: static }   /* LAST in the sheet — must lose */
```

So "echo the final matching declaration" and "report what the cascade decided"
give **different answers here**, and only the second is right:

```python
assert bordered["computed"]["box-sizing"] == "border-box", bordered["computed"]
bs = next(d for d in bordered["declared"] if d["property"] == "box-sizing")
assert bs["winner"]["selector"] == ".bordered", bs["winner"]
assert bs["winner"]["specificity"] == [0, 1, 0], bs["winner"]
assert len(bs["overridden"]) == 1, bs["overridden"]
assert bs["overridden"][0]["selector"] == "div", bs["overridden"]
assert bs["overridden"][0]["value"] == "content-box", bs["overridden"]
```

Second — and this is the half that makes them **count** — the reported value is
cross-checked against the geometry layout produced, which differs observably
between the two settings. `border-box` means the declared 200px **is** the
border box, so the content shrinks to 200 − 2·10 − 2·5 = 170 and 40 − 20 − 10 =
10; `.content` declares the same width, padding and border and differs only in
`box-sizing`, giving 200 + 2·10 + 2·5 = 230:

```python
assert bordered_box["border_box"]["height"] == 40.0, bordered_box["border_box"]
assert bordered_box["content_rect"]["width"] == 170.0, bordered_box["content_rect"]
assert bordered_box["content_rect"]["height"] == 10.0, bordered_box["content_rect"]
# ...and the control proves the engine is not simply always doing that:
assert content_box["content_rect"]["width"] == 200.0, content_box["content_rect"]
assert content_box["border_box"]["height"] == 70.0, content_box["border_box"]
```

`position` is cross-checked the same way, against **being in flow at all**:
`.host` is auto-height around a single 40px child, so a static child would make
it 40. Absolute takes the child out of flow, the parent collapses to 0, the
child is still really 40 tall, and the next block starts at the host's own y —
i.e. the 40px box reserved no space for itself.

```python
assert floater["computed"]["position"] == "absolute", floater["computed"]
assert pos["overridden"][0]["value"] == "static", pos["overridden"]   # the later rule
assert host_box["border_box"]["height"] == 0.0, host_box["border_box"]
assert host_box["children"][0]["border_box"]["height"] == 40.0, host_box["children"][0]
assert clipped_box["border_box"]["y"] == host_box["border_box"]["y"], \
    (clipped_box["border_box"], host_box["border_box"])
```

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
ok  computed expansion padding-left=16px, cited to `padding: 16px` on .hero (via_shorthand) — no rule spells the longhand
ok  origin split      h1 font-weight=700 winner=None (UA, no rule to cite); font-size=32px winner=h1 (author)
ok  line-height       .copy 20px x 1.5 = 30px — authored '1.5', computed 30px (resolved, not echoed)
ok  normal not faked  .hero line-height=normal (keyword, not px — resolving it needs font metrics)
ok  inherited origin  span font-family='Georgia, serif' text-align=center both origin=inherited; display still UA-or-initial
ok  KNOWN DIVERGENCE  span line-height reported 'normal' but laid out at 30px — inherited in build_layout_box, after the trace
ok  font-style        span computed=italic origin=inherited (declares nothing); paint drew it font_style=1, h1 still 0
ok  letter-spacing    .spaced 0.1em x 20px = 2px (authored '0.1em'); every advance is exactly +2.0 over .plain — [2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0]
ok  KNOWN DIVERGENCE  .pre keeps 'a  b' (4 advances) but its nested span collapsed 'c  d' to 'c d' — white-space is not inherited onto elements
ok  border shorthand  .framed border-top-width=5px (0.25em x 20px) and border-top-color=rgba(204, 102, 0, 1), both cited to `border` on .framed; layout reserved 5.0 and paint drew a 210.0x5.0 band
ok  no false citation `border: 2px solid` cites the width and NOT border-top-color (the declaration carried no colour)
ok  longhand wins     .overruled border-top-width=9px beats `border: 3px solid #093`, which is reported in overridden; layout reserved {'bottom': 3.0, 'left': 3.0, 'right': 3.0, 'top': 9.0} and a 52.0-tall box
ok  box-sizing        .bordered border-box: declared 200px IS the border box, content 170.0x10.0; .content content-box, same declarations, border box 230.0x70.0
ok  position          .floater absolute (beat `div{position:static}`, last in sheet); .host is 0.0 tall around a 40.0-tall child and the next block starts at the same y — out of flow
ok  KNOWN GAP        .clipped overflow-x=hidden cited to `overflow`, but it lays out identically to .unclipped (both 15.0 tall, kid at +10.0) and paint pushes no clip — reported, not counted
ok  KNOWN GAP        .faded opacity=0.5 computed, but paint filled it at a=1.0 — opacity never reaches the solid-colour path, so it is reported, not counted
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

**Checked that the gate can go red — three times, and the third is the point.**

1. Cascade drops `border-box` (`"border-box" => BoxSizing::ContentBox` in
   `apply_style_property`). The tool follows the engine rather than the
   stylesheet, and fails on the value:
   ```
   AssertionError: {... 'box-sizing': 'content-box', 'height': '40px', 'padding-left': '10px', ...}
   ```
2. Same cascade break, plus the **tool forced to report `border-box` anyway**
   (`"box-sizing" => "border-box".to_string()`). This is the failure clause 3
   exists to catch: a plausible value, correct provenance, and an engine that
   did something else. The value assertion passes; the cross-check catches it:
   ```
   AssertionError: no 200-wide box — expected .bordered (border-box)
   ```
   Without the layout half, that perturbation ships green. With it, the
   property genuinely counts.
3. Cascade drops `absolute` (`"absolute" => Position::Static`):
   ```
   AssertionError: {... 'position': 'static', 'width': '50px', ...}
   ```

All three reverted; `grep -r RED-CHECK crates/` returns nothing on the
committed tree.

### Two findings, which is why the box group is half-counted

Both properties parse, both reach `ComputedStyle`, and both now report a value
with honest provenance. Neither counts, because clause 3 asks whether the
engine **used** the value, and for these two nothing downstream does.

- **`overflow-x` changes nothing observable.** The citation half is genuinely
  good: no rule spells the longhand, and the winner names the `overflow`
  shorthand that really wrote it, measured by `longhands_written`. But
  `overflow_x` reaches layout only through `establishes_bfc`
  (`margin_collapse.rs`), and that distinction has no consequence in the block
  path — `.clipped` (`hidden`, a BFC) and `.unclipped` (`visible`, not one) lay
  identical children out identically: both 15 tall, both with the kid's 10px
  top margin retained inside. In a browser these differ, because a BFC parent
  keeps the child's margin in and a flow parent lets it collapse through.
  Nothing clips in paint either — **there is no `push_clip` for overflow at
  all**, only for `background-clip`. So `overflow: hidden` on a real page is
  currently decorative in RustKit, which is a rendering finding well beyond
  this export.
- **`opacity` never reaches paint.** The cascade clamps it to [0,1], so the
  reported `0.5` is computed rather than echoed. But paint reads `opacity` only
  on the `Image` command, and the renderer then **discards it** (`opacity: _`
  in the `DisplayCommand::Image` arm, `rustkit-renderer/src/lib.rs`). The
  `.faded` fill comes back at `a=1.0`, so a tool reporting 0.5 would be
  describing a transparency the engine does not draw.

Both are pinned by smoke tripwires that fail when the gap closes, in the shape
nights 6 and 7 used for `line-height` and `white-space`: whoever makes overflow
clip, or wires opacity into the solid-colour path, will see those assertions go
red and should then assert the new behaviour and **count** the property.

Per `BASELINE.md`'s scope limit I did **not** fix either. Both are rendering
changes that would move the parity corpus, and this is an export loop.

### What the engine still cannot answer

- **Four of twelve remain**, and the two cheap ones are gone. `overflow-x` and
  `opacity` are implemented-but-uncounted for the reasons above; `line-height`
  (night 6) and `white-space` (night 7) remain implemented-but-uncounted
  because they inherit **below** the cascade. Nothing tonight changes either
  pair. **All four now need an engine change, not an export change** — that is
  a real shift in the shape of the remaining work, and the next night should
  know it before starting.
- **`position` is answered for the value, not for the consequences.** `top`,
  `left`, `right`, `bottom`, `z-index` and the containing-block chain are not
  in the recorded set, so an agent can learn a box is absolute but not where it
  was told to go or what it was positioned against.
- **`overflow-y` is not exported**, only `overflow-x` — the diagnosis set names
  the x axis, and the shorthand writes both, so an agent asking about vertical
  clipping gets nothing. One table entry, no new mechanism, deliberately left
  so tonight's claim is exactly what was asserted.
- **Only the TOP border side is exported** (night 8's gap, unchanged), and
  **`border-style` is still invisible**, so `border: 3px none` and
  `border: 3px solid` report identically.
- **Non-px lengths still come back as Rust `Debug` strings** — visible in
  tonight's own red-check output as `'margin-top': 'Zero'`. Named by nights 6,
  7 and 8 as the cheapest correct thing left, still not done. It changes values
  the existing `hiwave_diff` references quote, so it wants its own slice.
- The fixture is now **66px from the viewport's 600px height** (body = 534),
  and two of tonight's red-checks initially failed on the pre-existing
  `800x600 canvas` assertion rather than on the property under test, because
  the perturbed page grew past the viewport. I shrank the new elements to buy
  headroom rather than touch that assertion. The next night that adds fixture
  elements will hit this — either budget height, or give the box group its own
  page.
- Everything trench 1 named is still true: `!important` is dead in the cascade
  (pinned by the `important-width` diff case), UA-origin properties have no
  rule to cite, `hiwave_style` takes simple selectors only, capture-kind
  references are refused, `style` is not a diffable stage, and there is no
  real-page corpus.

### Tests

`cargo test --workspace --no-fail-fast`, excluding `hiwave-app` and
`hiwave-smoke` (need GTK — `gdk-sys` build fails) and `rustkit-media` (needs
ALSA): **913 passed, 1 failed**. The one failure is
`rustkit-layout::probe_normal_line_height_vs_chrome` — the same pre-existing
failure nights 1-3, 7 and 8 reported, with the same `-apple-system` signature
(`14.00` against Chrome's `17.00`, `normal_line_height_probe`). It cannot be
tonight's: the only crate changed is `rustkit-engine`, and `rustkit-layout`
depends on `rustkit-dom`, `rustkit-css` and `rustkit-text` — not on
`rustkit-engine`, so this night's change cannot reach it.

`cargo build --release -p parity-capture` — the only thing CI actually builds —
finishes clean, so nothing here slips the workspace gate.

Scope stayed inside `crates/rustkit-engine/src/lib.rs` (three tables —
`COMPUTED_PROPERTIES`, `with_sentinel`, `computed_value_of`) and
`crates/hiwave-mcp/smoke.py`. **No engine behaviour changed:** the three tables
are read-only reporting paths, and `with_sentinel` only ever writes to
throwaway copies. No parity harness, no `.github/`, no Windows or Linux port
work, and no existing export altered — unlike night 8, this night supersedes
no prior assertion. The runner needed `mesa-vulkan-drivers` installed again
(fresh container; environment only, nothing committed).

### Decisions needed from Pete

1. **The nightly prompt still describes trench 1 — fourth night, and the
   failure mode is now demonstrated rather than hypothetical.** Tonight the
   scheduler's checkout was on `master`, where both `trench/digest.md` and
   `BASELINE.md` still say "4 of 4, stop condition met". Only night 8's warning
   block inside this branch's `BASELINE.md` prevented a wrongly-closed loop,
   and that block only helps a night that thinks to read the branch. Two ways
   out, both yours and both cheap: repoint the stored prompt at trench 2, or
   land this branch on master so a default checkout tells the truth. I cannot
   do either from inside a session — the routine was created via the API, and
   agents may only edit routines they created.

Next slice: the remaining four all need engine work before they can count, so
the next night should pick the ONE whose engine change is smallest and treat
the export as the easy half. My read: **`overflow-x`**, by making paint push a
clip for `overflow: hidden` — it is the only one of the four whose gap is also
a live rendering bug (`overflow: hidden` currently does not clip anything), so
the engine change pays for itself in the parity corpus rather than only in the
metric. That is a rendering change and therefore a corpus event, so it wants
Pete's nod before it starts, not after.

---

## 2026-08-07 — night 10 (`line-height`)

**Metric: 8 of 12 → 9 of 12**

**Moved no → yes: `line-height`.** It has been reported-but-uncounted since
night 6, and it did not need the engine change every prior night assumed.

### The stored prompt is still stale (fifth night), and the checkout was again master

Same as night 9, both halves. The firing described the metric as trench 1's
"four Tier-1 MCP reads" and told me to stop at 4 of 4; the scheduler's checkout
was on `master`, whose `trench/digest.md` ends at night 3 and whose
`BASELINE.md` has no trench 2 in it. Prompt and repo agreed the trench was
finished. Only fetching this branch disagreed. Night 8's warning block inside
this branch's `BASELINE.md` is again what prevented a wrongly-closed loop.
Decision below, unchanged and now five nights old.

### The finding that made it countable: it was never a rendering problem

Nights 6, 7 and 9 all concluded that `line-height` needed inheritance moved
into the cascade — a parity-corpus event — and correctly refused to do it
under this trench's scope limit. Night 9 went further and said all four
remaining properties "now need an engine change, not an export change."

That was right about `white-space`, `overflow-x` and `opacity`. It was wrong
about `line-height`, and the mistake is worth naming precisely, because it cost
four nights of the property sitting in the uncounted pile.

The divergence was never that layout used the wrong value. Layout used the
**right** value — it inherits line-height in `build_layout_box`, exactly as CSS
2.1 §10.8 says, and has since well before this trench. The divergence was that
the **trace was read too early**. `compute_style_for_element` snapshots the
record at its own end, and `build_layout_from_node_with_parent_style` then makes
two further adjustments to the very same `ComputedStyle` before handing it to
layout:

1. font-size absolutization — em/%/rem resolved against the parent;
2. line-height inheritance — the block quoted in night 6.

So the report was stale, not the engine. The fix is one call at the point where
the style is final:

```rust
// `style` is now the style LAYOUT is handed. [...] This changes NO engine
// behaviour — both adjustments above are untouched and predate this trench.
// Only what the trace says about them changes.
self.amend_trace_for_layout_style(&tag_lower, &style, parent_style);
```

`amend_trace_for_layout_style` re-derives the record's values and its
`inherited` labels through **the same two functions that produced them the
first time** (`computed_value_of`, `inherited_properties`), so it is a second
*reading* of the cascade's result and never a second opinion about it. It
writes nothing to `style`. No rendering path moves, so the parity corpus is
untouched and this stayed an export slice rather than the cascade surgery
`BASELINE.md` rules out.

### The assertion that proves it

The span declares nothing whatsoever — already asserted one line above, and
load-bearing here — so its 30px cannot be an echo of any declaration on the
element, and no UA default supplies it. It is `.copy`'s `font-size: 20px` and
`line-height: 1.5`, and 20 × 1.5 = 30, hand-derivable with no font metrics in
it (a numeric line-height overrides font metrics entirely).

```python
span_lh = next(x for x in span["declared"] if x["property"] == "line-height")
assert span_lh["computed"] == "30px", span_lh
assert span_lh["winner"] is None, span_lh          # no rule on this element
assert span_lh["origin"] == "inherited", span_lh   # ...and it says where it came from
```

That is clauses 1 and 2. Clause 3 — the value the tool reports must be the
value layout used — is what the second half is for, and the fixture already
had the geometry to prove it: **the `.copy` div's only text sits INSIDE the
span**, so the line box it generates is driven by the span's line-height and by
nothing else.

```python
inherited_text = find(tree["root"], lambda n: n.get("text") == "inherited")
assert inherited_text["rect"]["height"] == 30.0, inherited_text
# ...and the control, which proves 30.0 is not simply what any 20px text box
# measures here. `.plain` declares the same font-size and family but no
# line-height, so its line box is `normal` — a FONT-DERIVED number, asserted to
# DIFFER from 30 rather than pinned to a value.
plain_text = find(tree["root"], lambda n: n.get("text") == "spacing")
assert plain_text["rect"]["height"] != 30.0, plain_text
```

The control is deliberately asserted as `!= 30.0` and not as `== 20.0`. Its
height is whatever `normal` resolves to on this text stack, and pinning it
would make the gate report a platform difference as an engine regression —
which this file's own header rules out.

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
ok  computed expansion padding-left=16px, cited to `padding: 16px` on .hero (via_shorthand) — no rule spells the longhand
ok  origin split      h1 font-weight=700 winner=None (UA, no rule to cite); font-size=32px winner=h1 (author)
ok  line-height       .copy 20px x 1.5 = 30px — authored '1.5', computed 30px (resolved, not echoed)
ok  normal not faked  .hero line-height=normal (keyword, not px — resolving it needs font metrics)
ok  inherited origin  span font-family='Georgia, serif' text-align=center both origin=inherited; display still UA-or-initial
ok  line-height inherited  span declares nothing, reports 30px origin=inherited (.copy 20px x 1.5) and its line box IS 30.0 — `normal` control measures 20.0
ok  font-style        span computed=italic origin=inherited (declares nothing); paint drew it font_style=1, h1 still 0
ok  letter-spacing    .spaced 0.1em x 20px = 2px (authored '0.1em'); every advance is exactly +2.0 over .plain — [2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0]
ok  KNOWN DIVERGENCE  .pre keeps 'a  b' (4 advances) but its nested span collapsed 'c  d' to 'c d' — white-space is not inherited onto elements
ok  border shorthand  .framed border-top-width=5px (0.25em x 20px) and border-top-color=rgba(204, 102, 0, 1), both cited to `border` on .framed; layout reserved 5.0 and paint drew a 210.0x5.0 band
ok  no false citation `border: 2px solid` cites the width and NOT border-top-color (the declaration carried no colour)
ok  longhand wins     .overruled border-top-width=9px beats `border: 3px solid #093`, which is reported in overridden; layout reserved {'bottom': 3.0, 'left': 3.0, 'right': 3.0, 'top': 9.0} and a 52.0-tall box
ok  box-sizing        .bordered border-box: declared 200px IS the border box, content 170.0x10.0; .content content-box, same declarations, border box 230.0x70.0
ok  position          .floater absolute (beat `div{position:static}`, last in sheet); .host is 0.0 tall around a 40.0-tall child and the next block starts at the same y — out of flow
ok  KNOWN GAP        .clipped overflow-x=hidden cited to `overflow`, but it lays out identically to .unclipped (both 15.0 tall, kid at +10.0) and paint pushes no clip — reported, not counted
ok  KNOWN GAP        .faded opacity=0.5 computed, but paint filled it at a=1.0 — opacity never reaches the solid-colour path, so it is reported, not counted
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

**Checked that the gate can go red — three times, and the third is the point.**

1. Inheritance disabled in `build_layout_box` (`if false && matches!(...)`). The
   tool follows the engine rather than the stylesheet:
   ```
   AssertionError: {'computed': 'normal', 'origin': 'user-agent-or-initial',
                    'overridden': [], 'property': 'line-height', 'winner': None}
   ```
2. Same break, plus the report forced to claim the inheritance happened
   anyway — a plausible value AND correct-looking provenance, with an engine
   that did something else. This is the failure clause 3 exists to catch. Both
   the value and the origin assertions pass; only the geometry catches it:
   ```
   AssertionError: {'rect': {'height': 20.0, 'width': 90.0, 'x': 5.0, 'y': 152.0},
                    'text': 'inherited', 'type': 'text'}
   ```
   Without the layout half, that perturbation ships green. With it, the property
   genuinely counts.
3. The new unit test, with the amend call commented out:
   ```
   assertion `left == right` failed
     left: "normal"
    right: "30px"
   ```

All three reverted; `grep -rn RED-CHECK crates/` returns nothing on the
committed tree.

### A second, smaller correction rode along — and is NOT claimed

The same staleness hid a second wrong value: an em/%/rem `font-size` was
reported as an unresolved Rust `Debug` string (`Em(2.0)`) because
absolutization also happens after the snapshot. That is now correct too — the
new unit test pins `2em` against a 20px parent reporting `40px`, which is where
it is verified, because the smoke fixture has no em font-size to assert against.

I am **not** counting it and it is not in the diagnosis set. Flagging it because
it is a behaviour change in an export that already had passing assertions, and
the trench's rule is that those are left alone: nothing that previously passed
changed its answer (the fixture declares every font-size in px), but a caller
outside this fixture will now get `40px` where it used to get `Em(2.0)`. That is
strictly more correct and it is the direction the "non-px lengths come back as
Debug strings" gap has been asking for since night 6 — but it is a partial step
into that gap, not the slice that closes it.

### What the engine still cannot answer

- **Three of twelve remain, and night 9's read of them stands — for these
  three.** `white-space`, `overflow-x` and `opacity` all genuinely need an
  engine change, and none of them has the shape `line-height` turned out to
  have. `white-space` is not inherited onto elements at all (paint shows a
  nested span collapsing spaces its `pre` parent kept), `overflow: hidden`
  pushes no clip anywhere in paint, and `opacity` is discarded by the renderer
  outside the `Image` arm. In each case layout/paint uses a value that is
  **wrong**, so no amount of better reporting can make the tool agree with a
  correct engine. Tonight's move does not generalise to them, and a night that
  assumes it does will waste itself.
- **`line-height: normal` is still reported as the keyword, not resolved to
  px.** Deliberate and unchanged: resolving it needs the font's ascent/descent/
  line-gap, so a number there would look machine-independent and would not be.
  An agent asking "how tall is this line box" for a `normal` element still gets
  `normal` and must read the layout tree instead. Pinned by the untouched
  `normal not faked` assertion.
- **The amendment covers the two adjustments that exist today, and is pinned to
  them by a tag guard, not by a general mechanism.** If a future edit adds a
  third post-cascade adjustment to `build_layout_from_node_with_parent_style`,
  the trace goes stale again for that property and nothing will say so. The
  honest description is "the trace is re-read at the one point the style is
  final", and that point is a line in a function, not an invariant the type
  system holds.
- **Non-px lengths still come back as Rust `Debug` strings** for everything
  except font-size — `'height': 'Auto'`, `'margin-top': 'Zero'`. Named by
  nights 6, 7, 8 and 9 as the cheapest correct thing left, and tonight made the
  inconsistency *within one payload* slightly worse rather than better, since
  font-size now resolves and its neighbours do not. Still wants its own slice
  and its own red-check, because it changes values the existing `hiwave_diff`
  references quote.
- **Only the TOP border side is exported**, **`border-style` is still
  invisible**, **`overflow-y` is not exported**, and **`position` is answered
  for the value, not the consequences** (`top`/`left`/`z-index`/containing
  block are not recorded). All carried unchanged from nights 8 and 9.
- **The fixture is 66px from the viewport's 600px height** (body = 534,
  unchanged tonight — this slice added no elements). The next night that adds
  any will hit it.
- Everything trench 1 named is still true: `!important` is dead in the cascade
  (pinned by the `important-width` diff case), UA-origin properties have no rule
  to cite, `hiwave_style` takes simple selectors only, capture-kind references
  are refused, `style` is not a diffable stage, and there is no real-page
  corpus.

### Tests

`cargo test --workspace --no-fail-fast`, excluding `hiwave-app` and
`hiwave-smoke` (need GTK — `gdk-sys` build fails) and `rustkit-media` (needs
ALSA): **914 passed, 1 failed** — 913 from night 9 plus the one new unit test.

The one failure is `rustkit-layout::probe_normal_line_height_vs_chrome`, the
same pre-existing failure nights 1–3 and 7–9 reported, with the same
`-apple-system` signature (`14.00` against Chrome's `17.00`). It deserves more
than the usual note this night, because it is a **line-height** test and this
was a line-height slice: confirmed structurally rather than by assertion —
`rustkit-layout`'s dependencies are `rustkit-dom`, `rustkit-css` and
`rustkit-text`, so it does not depend on `rustkit-engine`, which is the only
crate this night changed outside the smoke script. It is also about `normal`
resolution from font metrics, which is the one thing tonight deliberately did
not touch.

`cargo build --release -p parity-capture` — the only thing CI actually builds —
**finishes clean** — `Finished release profile [optimized] target(s) in 4m 03s`
— so nothing here slips the workspace gate.

`cargo fmt --check -p rustkit-engine` reports **22 diffs both with and without
this night's change** — i.e. tonight added none. All 22 are pre-existing on
HEAD. I did not run `cargo fmt`, because it would reformat unrelated code and
CI gates neither fmt nor clippy.

Scope stayed inside `crates/rustkit-engine/src/lib.rs` and
`crates/hiwave-mcp/smoke.py`. No parity harness, no `.github/`, no Windows or
Linux port work, no engine behaviour changed, and no fixture change. The one
edit to an existing assertion is night 6's `line-height` tripwire, which is
exactly the edit that tripwire was written to invite ("whoever moves that
inheritance will see this go red, and should assert `30px` and count the
property, not route around it") — with the correction that no one had to move
the inheritance. The runner needed `mesa-vulkan-drivers` installed again
(fresh container; `apt-get update` first, since the cached index 404s;
environment only, nothing committed).

### Decisions needed from Pete

1. **The nightly prompt still describes trench 1 — fifth night.** Unchanged
   from nights 7, 8 and 9, and I cannot fix it from inside a session (the
   routine was created via the API; agents may only edit routines they
   created). Two cheap ways out, both yours: repoint the stored prompt at
   trench 2, or land this branch on master so a default checkout stops saying
   the trench is finished. Tonight is the fifth night that spent its first move
   deciding whether to believe the prompt or the repo, and the failure mode is
   silent — a night that believes the prompt reports a completed trench and
   does nothing.
2. **The three remaining properties need rendering changes, and that is now the
   whole of the remaining work.** `white-space` inheritance, `overflow: hidden`
   clipping, and `opacity` reaching the solid-colour path are each a real
   rendering bug with parity-corpus consequences, and `BASELINE.md`'s scope
   limit correctly forbids this loop from fixing them. So the trench is one
   slice from its floor: unless that limit is relaxed or the work is handed to
   the parity trench, the next two nights are NONE and the loop ends at 9 of 12
   on the two-dry-nights clause. That is a legitimate ending and I am not
   arguing against it — but it is now predictable, so it is better decided than
   discovered.

Next slice: there is no export-only slice left in the diagnosis set. If the
answer to decision 2 is "leave them", the honest next action is the funeral
note at 9 of 12 rather than two nights of going through the motions. If a
rendering change is authorised, `overflow-x` remains the best-value one for the
reason night 9 gave — it is the only one of the three whose gap is also a live
rendering bug that pays for itself in the parity corpus — and the export half is
already built and reporting.

## 2026-08-08 — night 11 (`overflow-x`)

**Metric: 9 of 12 → 10 of 12**

**Moved no → yes: `overflow-x`.** Like `line-height` on night 10, it did not
need the engine change the previous two nights assumed — and for the same
underlying reason: the claim "the engine does not use this value" had been
tested in the one place the engine does not use it.

### The stored prompt is still stale (sixth night), and the checkout was again master

Unchanged from nights 7–10. The firing described the metric as trench 1's "four
Tier-1 MCP reads", told me the four were `hiwave_layout`, `hiwave_display_list`,
`hiwave_style`, `hiwave_diff`, and instructed me to stop and write a completion
note if the digest already showed 4 of 4 — which `master`'s copy of this file
does, because it ends at night 3. Following the prompt literally would have
closed a live trench at the first read. `BASELINE.md`'s warning block on this
branch is again the only thing that prevented it. Decision below, now six nights
old.

### The finding that made it countable: night 9 measured the wrong margin

Night 9 put `overflow-x` in the reported-but-uncounted pile with a specific
justification, which nights 10 and this file's own smoke comment both repeated:
`overflow-x` reaches layout only through `establishes_bfc`, and a BFC parent and
a flow parent "lay out identically here". The fixture backed it up — `.clipped`
and `.unclipped` were both 15.0 tall with their kid at +10.0.

That was true of the fixture and false of the engine. The fixture's `.kid` had a
`margin-top`, and **top-margin collapse is not wired**: `margin_collapse.rs`
exports `should_collapse_with_first_child`, `rustkit-layout/src/lib.rs` imports
it, and nothing ever calls it. So the top margin stays inside whatever the
parent is, BFC or not, and the two boxes were bound to agree.

`should_collapse_with_last_child` **is** called, once, at the end of block
layout, and it asks `establishes_bfc`, which reads `style.overflow_x` directly:

```rust
// margin_collapse.rs
if style.overflow_x != Overflow::Visible || style.overflow_y != Overflow::Visible {
    return true;
}
```

```rust
// rustkit-layout/src/lib.rs — the only caller in the block path
if !should_collapse_with_last_child(
    &self.style, self.float,
    self.dimensions.border.bottom, self.dimensions.padding.bottom,
) {
    cursor_y += margin_context.resolve();   // the pending bottom margin lands INSIDE
    margin_context.reset();
}
```

So the value is spent on the **last child's bottom margin**, not the first
child's top margin. One word in the fixture (`margin-bottom: 20px` on the
shared `.kid`) makes it observable. No engine behaviour was changed tonight;
the export already reported the value correctly, and what was missing was proof
that layout consumed it.

### The assertion that proves it

`.clipped` and `.unclipped` hold the **same** `.kid` and differ in exactly one
declaration — `overflow: hidden`. The kid is `margin-top: 10px; height: 5px;
margin-bottom: 20px`, so both numbers are hand-derivable with no font metrics
in them:

- `.clipped` — `overflow: hidden` → a BFC → no collapse-through → the pending
  20px bottom margin is materialised inside the parent: 10 + 5 + 20 = **35**
- `.unclipped` — `visible` → not a BFC → collapse-through allowed → the pending
  margin is not added: 10 + 5 = **15**

Clauses 1 and 2 (computed value, and not an echo — `overflow-x` is a longhand no
rule spells, written by the `overflow` shorthand):

```python
assert clipped["computed"]["overflow-x"] == "hidden", clipped["computed"]
ox = next(d for d in clipped["declared"] if d["property"] == "overflow-x")
assert ox["winner"]["property"] == "overflow", ox["winner"]
assert ox["winner"]["via_shorthand"] is True, ox["winner"]
assert ox["winner"]["selector"] == ".clipped", ox["winner"]
```

Clause 3 — the reported value is the value **layout used** — is the half that
makes it count, and it is 20.0 of geometry attributable to one keyword:

```python
assert clipped_h == 35.0, clipped_box["border_box"]
assert unclipped_h == 15.0, unclipped_box["border_box"]
# ...and the gap is exactly the kid's bottom margin, not some other drift:
# the two children are identical, so anything else differing would show up here.
assert clipped_h - unclipped_h == 20.0, (clipped_h, unclipped_h)
```

**What is deliberately NOT claimed: neither number is Chrome's.** Chrome
collapses the top margin out as well (the engine does not — the unwired helper
above) and adjoins the escaping bottom margin to the parent rather than dropping
it (ledgered in the block-height comment as a known residual). Both are
pre-existing and unrelated to this property. What is asserted is the engine's
own behaviour, and specifically the difference `overflow-x` alone causes.

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
ok  computed expansion padding-left=16px, cited to `padding: 16px` on .hero (via_shorthand) — no rule spells the longhand
ok  origin split      h1 font-weight=700 winner=None (UA, no rule to cite); font-size=32px winner=h1 (author)
ok  line-height       .copy 20px x 1.5 = 30px — authored '1.5', computed 30px (resolved, not echoed)
ok  normal not faked  .hero line-height=normal (keyword, not px — resolving it needs font metrics)
ok  inherited origin  span font-family='Georgia, serif' text-align=center both origin=inherited; display still UA-or-initial
ok  line-height inherited  span declares nothing, reports 30px origin=inherited (.copy 20px x 1.5) and its line box IS 30.0 — `normal` control measures 20.0
ok  font-style        span computed=italic origin=inherited (declares nothing); paint drew it font_style=1, h1 still 0
ok  letter-spacing    .spaced 0.1em x 20px = 2px (authored '0.1em'); every advance is exactly +2.0 over .plain — [2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0]
ok  KNOWN DIVERGENCE  .pre keeps 'a  b' (4 advances) but its nested span collapsed 'c  d' to 'c d' — white-space is not inherited onto elements
ok  border shorthand  .framed border-top-width=5px (0.25em x 20px) and border-top-color=rgba(204, 102, 0, 1), both cited to `border` on .framed; layout reserved 5.0 and paint drew a 210.0x5.0 band
ok  no false citation `border: 2px solid` cites the width and NOT border-top-color (the declaration carried no colour)
ok  longhand wins     .overruled border-top-width=9px beats `border: 3px solid #093`, which is reported in overridden; layout reserved {'bottom': 3.0, 'left': 3.0, 'right': 3.0, 'top': 9.0} and a 52.0-tall box
ok  box-sizing        .bordered border-box: declared 200px IS the border box, content 170.0x10.0; .content content-box, same declarations, border box 230.0x70.0
ok  position          .floater absolute (beat `div{position:static}`, last in sheet); .host is 0.0 tall around a 40.0-tall child and the next block starts at the same y — out of flow
ok  overflow-x       .clipped overflow-x=hidden cited to `overflow`; it establishes a BFC so the kid's 20px bottom margin lands INSIDE — 35.0 tall against 15.0 for .unclipped, same child, one keyword apart
ok  KNOWN GAP        ...but paint pushes no clip for it — overflow-x is answered for what LAYOUT did, not for whether anything was clipped
ok  KNOWN GAP        .faded opacity=0.5 computed, but paint filled it at a=1.0 — opacity never reaches the solid-colour path, so it is reported, not counted
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

**Checked that the gate can go red — and the interesting part is where it
failed.** The overflow arm of `establishes_bfc` was disabled (`if false && ...`),
so the engine stops spending the value while the cascade still computes it
perfectly:

```
AssertionError: {'height': 15.0, 'width': 260.0, 'x': 0.0, 'y': 494.0}
  crates/hiwave-mcp/smoke.py:737, in main
    assert clipped_h == 35.0, clipped_box["border_box"]
```

It failed at the **geometry**, line 737 — every assertion above it (computed
`hidden`, winner cited to `overflow`, `via_shorthand: true`) still passed. That
is the exact failure clause 3 exists to catch: a plausible value with correct
provenance, over an engine doing something else. Without the layout half this
perturbation ships green. Perturbation reverted; `grep -rn RED-CHECK crates/`
returns 0 on the committed tree.

### What the engine still cannot answer

- **`overflow-x` is answered for what LAYOUT did, NOT for whether anything was
  clipped.** Paint emits no `push_clip` for overflow anywhere — the tripwire
  asserting that is kept, not deleted — so an agent reading `hidden` learns
  which margin-collapse branch was taken and must not conclude the content was
  clipped. This is the same shape as night 9's `position`, counted "for the
  value, not the consequences", and it is published in the payload's `limits`
  rather than left for a reader to discover. Whoever implements clipping will
  see that tripwire go red and should assert the clip instead of removing the
  line.
- **`overflow-y` is still not exported at all**, so `overflow: hidden` on the
  block axis is invisible even though `establishes_bfc` reads it too.
- **`white-space` — verified in code tonight, and night 10's read stands.**
  `build_layout_box` seeds `white_space` from the parent only in the
  `NodeType::Text` arm (`s.white_space = parent.white_space`, alongside the
  other text properties); ELEMENTS are never seeded, so a `<span>` inside a
  `pre` ancestor genuinely lays out `normal` and the paint tripwire showing its
  collapsed `'c d'` is correct. There is no second route like the one
  `overflow-x` turned out to have: the only derivation that would satisfy
  clause 2 is inheritance, and inheritance is precisely what is missing.
  Counting it requires seeding the property in the element cascade — a
  rendering change, which `BASELINE.md` forbids this loop.
- **`opacity` — unchanged and genuinely blocked.** The cascade computes and
  clamps it, but the renderer discards it outside the `Image` arm (`opacity: _`),
  so `.faded` is still filled at `a=1.0`. Clause 2 has no route either: opacity
  is not inherited and no shorthand sets it, so its computed value can only ever
  echo the declaration text. It needs the renderer honouring it.
- **The fixture is now 46px from the viewport's 600px height** (body = 554, up
  from 534 — this slice spent 20 of the 66 night 10 reported). The next night
  that adds elements will hit it.
- **Non-px lengths still come back as Rust `Debug` strings** for everything
  except font-size (`'height': 'Auto'`, `'margin-top': 'Zero'`). Named by nights
  6–10; still the cheapest correct thing left, still wants its own slice and its
  own red-check because it changes values the `hiwave_diff` references quote.
- **Only the TOP border side is exported**, **`border-style` is invisible**, and
  **`position` is answered for the value, not the consequences** (`top`/`left`/
  `z-index`/containing block are not recorded). Carried from nights 8 and 9.
- Everything trench 1 named is still true: `!important` is dead in the cascade
  (pinned by the `important-width` diff case), UA-origin properties have no rule
  to cite, `hiwave_style` takes simple selectors only, capture-kind references
  are refused, `style` is not a diffable stage, and there is no real-page corpus.

### Tests

`cargo test --workspace --no-fail-fast`, excluding `hiwave-app` and
`hiwave-smoke` (need GTK — `gdk-sys` build fails) and `rustkit-media` (needs
ALSA): **914 passed, 1 failed** — the same totals as night 10, which is expected
since this night added no unit test (the proof is a smoke assertion over real
layout, which is where the value is spent).

The one failure is `rustkit-layout::probe_normal_line_height_vs_chrome`, the
same pre-existing failure nights 1–3 and 7–10 reported, with the same
`-apple-system` signature (`14.00` against Chrome's `17.00`). This night touched
`rustkit-layout` only during the red-check perturbation, which was reverted and
verified reverted (`git status` shows the crate clean); the committed change is
confined to `crates/rustkit-engine/src/lib.rs` and `crates/hiwave-mcp/smoke.py`.

The engine edit is comments and two new `limits` strings — **no executable
change**. The export already served `overflow-x` correctly; what changed is that
it is now cross-checked, and that the payload stops describing it as unusable.

Scope stayed inside `crates/hiwave-mcp/` and `crates/rustkit-engine/`. No parity
harness, no `.github/`, no Windows or Linux port work, no engine behaviour
changed, and no export that already had passing assertions had its answers
altered. The runner needed `mesa-vulkan-drivers` installed again (fresh
container; `apt-get update` first, since the cached index 404s; environment
only, nothing committed).

### Decisions needed from Pete

1. **The nightly prompt still describes trench 1 — sixth night.** Unchanged from
   nights 7–10, and I still cannot fix it from inside a session (the routine was
   created via the API; agents may only edit routines they created). Two cheap
   fixes, both yours: repoint the stored prompt at trench 2, or land this branch
   on master so a default checkout stops saying the trench is finished. Tonight
   this cost more than the usual first move — the prompt's stop condition
   explicitly matched what `master`'s digest says, so the instructed action was
   to write a completion note and stop.
2. **Two properties remain and both need rendering changes** — `white-space`
   inheritance onto elements, and `opacity` reaching the solid-colour path. Both
   were re-verified in code tonight rather than inherited from the previous
   night's read, and neither has a second route like the one `overflow-x` turned
   out to have. So night 10's endgame still holds, one property later: unless
   the scope limit is relaxed or the work is handed to the parity trench, the
   next two nights are NONE and the loop ends at 10 of 12 on the two-dry-nights
   clause. Worth deciding rather than discovering — with the caveat that two
   nights running have now overturned a confident "this needs engine surgery",
   so I would not bet heavily against a third.

Next slice: there is no export-only slice left in the diagnosis set that I can
see, and tonight is the second night in a row that sentence turned out to be
worth testing anyway. If a rendering change is authorised, `opacity` is the
smaller of the two (one arm in the renderer, and the export half is already
built and asserted); `white-space` is the more valuable, because its absence is
visible in the parity corpus as collapsed text inside `pre`.

---

## 2026-08-09 — night 12 (NONE — `white-space` and `opacity` both re-tested)

**Metric: 10 of 12 → 10 of 12**

**Moved no → yes: NONE.** This is the **first** dry night of trench 2. Night 11
moved `overflow-x`, so the two-dry-nights clause is not tripped; one more NONE
ends the trench and the funeral note is next night's job, not tonight's.

### Why I went looking anyway

Nights 10 and 11 each overturned a confident "this needs engine surgery", and
night 11 said it "would not bet heavily against a third". So the two remaining
properties were re-tested from the source rather than inherited from the previous
read. Both are genuinely blocked — but the *reasons* are sharper than night 11
had, and one of them is a new bug.

### `white-space`: the gap is bigger than "inheritance is missing"

Night 11 named the blocker as absent ELEMENT inheritance. That is real, and
re-confirmed: `build_layout_box` seeds `white_space` from `parent_style` only in
the `NodeType::Text` arm (lib.rs:1954), so a `<span>` inside a `pre` genuinely
lays out `normal`.

**But there is a second, independent gap, and it is the one real pages hit:
`<pre>` has no UA `white-space` at all.** The UA arm sets display, font-family
and margins and then says so in the source:

```rust
"pre" => {
    style.display = rustkit_css::Display::Block;
    style.font_family = "monospace".to_string();
    style.margin_top = rustkit_css::Length::Px(16.0); // 1em
    style.margin_bottom = rustkit_css::Length::Px(16.0);
    // white-space: pre (not implemented)
}
```

So a bare `<pre>` collapses its whitespace like a div. Every prior white-space
assertion needed an author `white-space: pre` to see anything, which is why six
nights of work on this property never surfaced it — the fixture always supplied
what the UA sheet does not.

**Pinned with a tripwire**, which is what `BASELINE.md` asks for when the finding
cannot be fixed inside this loop:

```python
bare_pre, error = client.tool("hiwave_style", selector="pre")
assert bare["computed"]["white-space"] == "normal", bare["computed"]
assert bare_ws["winner"] is None, bare_ws
assert bare_ws["origin"] == "user-agent-or-initial", bare_ws
# ...and the UA sheet DID reach this element for other properties, so `normal`
# is a missing UA declaration and not a UA sheet that missed the element.
assert bare["computed"]["display"] == "block", bare["computed"]
# The consequence, in paint: the double space is gone.
assert "x y" in texts and "x  y" not in texts, sorted(texts)
assert len(texts["x y"]["advances"]) == 3, texts["x y"]
```

**This is still NOT countable, and the reason is now exhaustive rather than
partial.** There are exactly three routes to a white-space value that is not an
echo of declaration text, and all three are absent:

1. **a shorthand** — nothing sets it; `shorthands_setting` maps only the `font`
   family (`line-height | font-family | font-size | font-weight => &["font"]`);
2. **inheritance** — absent for elements (night 11, re-verified);
3. **a UA default** — absent, above.

`normal` on a bare `<pre>` is the INITIAL value, so asserting it proves the
engine defaulted rather than computed. Clause 3 holds; clause 2 has nowhere to
come from.

### `opacity`: blocked, and night 11's parenthetical was slightly wrong

Night 11 wrote that the renderer "discards it outside the `Image` arm
(`opacity: _`)". The `opacity: _` it cites is **inside** the Image arm — which
matters, because as written it reads as though images honour opacity. They do
not. Traced the whole path tonight:

- `DisplayCommand::Image.opacity` **is** populated from the cascade —
  `rustkit-layout/src/lib.rs:4925` passes `layout_box.style.opacity` into
  `images::render_image`, and `hiwave_display_list` exports it. So the value
  reaches paint's input.
- The renderer's own Image arm then drops it: `DisplayCommand::Image { opacity: _, .. } => self.draw_image(url, *dest_rect)`.
- **Not a route:** the text path. `rustkit-renderer` line ~4517 preserves "the
  run's alpha for opacity/fade", but that alpha is `color.a` — the text colour's
  own alpha, not `style.opacity`.
- **Not a route:** layer creation. `BASELINE.md` lists `opacity` as deciding
  "whether a layer is created", and in this engine it does not — `opacity` never
  reaches `rustkit-compositor` at all (it appears in `rustkit-animation`,
  `-css`, `-engine`, `-layout`, `-renderer`, `-svg`, and nowhere else).
- **Not a route:** SVG. `rustkit-svg`'s `opacity`/`fill_opacity`/`stroke_opacity`
  are SVG presentation attributes on SVG nodes, not the CSS cascade's field.

So the only place the computed value is plumbed is a display-list field its sole
consumer ignores. Clause 2 could be met (the cascade clamps, so `opacity: -0.5`
computes `0` — not an echo), but clause 3 cannot: there is no consumer whose
behaviour differs as a function of the value, so an assertion could only prove
the cascade agrees with itself. That is a gate that cannot go red on anything
except the cascade, and this trench does not count those.

### The run

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
ok  computed expansion padding-left=16px, cited to `padding: 16px` on .hero (via_shorthand) — no rule spells the longhand
ok  origin split      h1 font-weight=700 winner=None (UA, no rule to cite); font-size=32px winner=h1 (author)
ok  line-height       .copy 20px x 1.5 = 30px — authored '1.5', computed 30px (resolved, not echoed)
ok  normal not faked  .hero line-height=normal (keyword, not px — resolving it needs font metrics)
ok  inherited origin  span font-family='Georgia, serif' text-align=center both origin=inherited; display still UA-or-initial
ok  line-height inherited  span declares nothing, reports 30px origin=inherited (.copy 20px x 1.5) and its line box IS 30.0 — `normal` control measures 20.0
ok  font-style        span computed=italic origin=inherited (declares nothing); paint drew it font_style=1, h1 still 0
ok  letter-spacing    .spaced 0.1em x 20px = 2px (authored '0.1em'); every advance is exactly +2.0 over .plain — [2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0]
ok  KNOWN DIVERGENCE  .pre keeps 'a  b' (4 advances) but its nested span collapsed 'c  d' to 'c d' — white-space is not inherited onto elements
ok  KNOWN GAP        a BARE <pre> computes white-space=normal (winner=None) and collapses 'x  y' to 'x y' — the UA sheet reached it (display=block, monospace) but has no white-space declaration to give
ok  border shorthand  .framed border-top-width=5px (0.25em x 20px) and border-top-color=rgba(204, 102, 0, 1), both cited to `border` on .framed; layout reserved 5.0 and paint drew a 210.0x5.0 band
ok  no false citation `border: 2px solid` cites the width and NOT border-top-color (the declaration carried no colour)
ok  longhand wins     .overruled border-top-width=9px beats `border: 3px solid #093`, which is reported in overridden; layout reserved {'bottom': 3.0, 'left': 3.0, 'right': 3.0, 'top': 9.0} and a 52.0-tall box
ok  box-sizing        .bordered border-box: declared 200px IS the border box, content 170.0x10.0; .content content-box, same declarations, border box 230.0x70.0
ok  position          .floater absolute (beat `div{position:static}`, last in sheet); .host is 0.0 tall around a 40.0-tall child and the next block starts at the same y — out of flow
ok  overflow-x       .clipped overflow-x=hidden cited to `overflow`; it establishes a BFC so the kid's 20px bottom margin lands INSIDE — 35.0 tall against 15.0 for .unclipped, same child, one keyword apart
ok  KNOWN GAP        ...but paint pushes no clip for it — overflow-x is answered for what LAYOUT did, not for whether anything was clipped
ok  KNOWN GAP        .faded opacity=0.5 computed, but paint filled it at a=1.0 — opacity never reaches the solid-colour path, so it is reported, not counted
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

**Checked that the new tripwire can go red**, by implementing the one missing
line (`style.white_space = rustkit_css::WhiteSpace::Pre` in the `"pre"` UA arm):

```
AssertionError: {... 'display': 'block', 'font-family': 'monospace',
                'white-space': 'pre', ...}
  crates/hiwave-mcp/smoke.py:533, in main
    assert bare["computed"]["white-space"] == "normal", bare["computed"]
```

and the paint half flips with it — the bare `<pre>` then keeps both spaces,
`text='x  y' advances=4` against tonight's `'x y'`/3. That is worth stating
plainly: **the whole bug is that one absent declaration**, not a chain of
missing machinery. Perturbation reverted; `grep -c RED-CHECK` on
`rustkit-engine/src/lib.rs` returns 0 and `git status` shows the crate clean.

### A process note, because it cost most of the night

The stored prompt is stale for the seventh night, and this time it did more than
waste the first move. The session started on a **detached HEAD at master**, where
`git branch -r` listed only `origin/master` — the clone's refspec does not fetch
this branch — so the branch looked absent, `trench/digest.md` read as ending at
night 3, and both the prompt's stop condition and the file agreed the trench was
finished at 4 of 4. I wrote and committed that completion note before
`git push` rejected it and revealed 62 commits of trench 2. The commit was
discarded, not pushed. `BASELINE.md`'s warning block did not help here, because
I was reading master's copy of it, which predates the warning.

The cheap durable fix is unchanged and is still Pete's: land this branch on
master, or repoint the prompt. Until then the reliable first move is
`git ls-remote origin 'refs/heads/atlas/*'` rather than `git branch -r`, which is
now in the note below for the next night.

Second, smaller: **the fixture hit night 11's viewport ceiling, exactly as
predicted.** A bare `<pre>` with UA margins (16 + 16 + a monospace line) does not
fit in the 46px that were left, and the failure was the canvas assertion
(`expected one 800x600 canvas fill`), not anything about white-space. `pre
{ margin: 0 }` buys it back; that rule is viewport bookkeeping and not part of
the finding. **The fixture is now roughly 28px from 600.** The next night that
adds an element must either shrink something or open the page taller — this is
the last night that can be solved by trimming a margin.

### What the engine still cannot answer

- **`white-space` — three named routes, all absent** (shorthand, element
  inheritance, UA default). Both halves are now pinned by tripwires rather than
  one. Counting it needs either the UA declaration or element inheritance, both
  rendering changes; and note that even with the UA default it is not obviously
  countable, because a UA keyword default is still not a computed value under
  clause 2.
- **`opacity` — plumbed to exactly one display-list field whose only consumer
  ignores it**, with the text-alpha, compositor and SVG routes all ruled out
  tonight. Needs the renderer honouring it.
- **`overflow-y` is still not exported**, and `overflow-x` is still answered for
  what layout did, not for whether anything was clipped.
- **Non-px lengths still come back as Rust `Debug` strings** — visible in
  tonight's own red-check output (`'height': 'Auto'`, `'margin-top': 'Zero'`,
  `'padding-left': 'Zero'`). Named by nights 6–11; still the cheapest correct
  thing left, still wants its own slice and its own red-check, because it changes
  values the `hiwave_diff` references quote.
- **Only the TOP border side is exported**, `border-style` is invisible, and
  `position` is answered for the value and not the consequences (`top`/`left`/
  `z-index`/containing block unrecorded).
- Everything trench 1 named is still true: `!important` is dead in the cascade
  (pinned by the `important-width` diff case), UA-origin properties have no rule
  to cite, `hiwave_style` takes simple selectors only, capture-kind references
  are refused, `style` is not a diffable stage, and there is no real-page corpus.

### Tests

**No Rust changed tonight** — the committed diff is `crates/hiwave-mcp/smoke.py`
only, a Python file that nothing compiles. `cargo test -p rustkit-engine`
(the crate whose behaviour the new assertion describes) — **36 passed, 0
failed**. I did not re-run the full workspace, because no compiled input to it
changed; nights 7–11's standing result (914 passed, 1 pre-existing
`rustkit-layout::probe_normal_line_height_vs_chrome` failure) is unaffected by a
test-script edit. The red-check perturbation touched `rustkit-engine` and was
reverted and verified reverted before the commit.

Scope stayed inside `crates/hiwave-mcp/`. No parity harness, no `.github/`, no
Windows or Linux port work, no engine behaviour changed, and no export that
already had passing assertions had its answers altered. The runner needed
`mesa-vulkan-drivers` installed again (fresh container; `apt-get update` first —
the cached index now 403s through the proxy rather than 404s; environment only,
nothing committed).

### Decisions needed from Pete

1. **One line in the UA sheet fixes bare `<pre>`, and I am not allowed to write
   it.** `style.white_space = WhiteSpace::Pre` in the `"pre"` arm; verified
   tonight to be the entire fix, with the paint consequence flipping. This is a
   parity-visible correctness bug on a plain HTML element, and it is a rendering
   change, which `BASELINE.md` forbids this loop. Authorise it here, or hand it
   to the parity trench — but it should not sit behind an export metric.
2. **The stored prompt, seventh night, now with a concrete cost.** It caused a
   real (discarded) commit closing a live trench tonight, because on master both
   the prompt and the digest say 4 of 4. Land this branch or repoint the prompt.
3. **The trench is one NONE from ending.** Both remaining properties need
   rendering changes and tonight's re-test made those reasons exhaustive rather
   than provisional. If `opacity` or `white-space` is not authorised as a
   rendering change, next night is NONE and the correct action is the funeral
   note at 10 of 12. Worth deciding rather than discovering.

**Next night, first move:** `git ls-remote origin 'refs/heads/atlas/*'` and check
out `atlas/trench-mcp-exports` BEFORE reading `trench/`, then rebuild
(`cargo build -p hiwave-mcp`) — a stale `target/` from master silently fails
night 8's provenance assertions and looks like a regression. Then: unless a
rendering change is authorised, write the funeral note at 10 of 12.

---

## 2026-08-10 — night 13 (`white-space`, on night 9's standard)

**Metric: 10 of 12 → 11 of 12**

**Moved no → yes: `white-space`.**

This one is a judgment call rather than new machinery, so the reasoning comes
before the assertion, and Pete has a decision on it below.

### Why night 12 was wrong to leave it uncounted

Night 12 marked `white-space` uncountable and gave an exhaustive reason: the
three routes to a value that is not an echo of declaration text — a shorthand,
element inheritance, a UA default — are all absent. **Every word of that is
still true**, it was re-verified tonight, and both gaps stay pinned as
tripwires.

But it is not the test under which two keyword properties were already counted.
Night 9 hit this exact problem and settled it in its own words: *"Both
properties are keywords, so no unit conversion or shorthand expansion can show
the value was computed rather than echoed. Two other things do it."* Those two:

- **(a)** a **later, LOWER-specificity** rule matching the same element declares
  the other value and loses — so "echo the last matching declaration" and
  "report what the cascade decided" give **different answers**, and only the
  second is right;
- **(b)** the reported value is cross-checked against a consequence the engine
  produced, which differs observably between the two settings.

`box-sizing` and `position` counted on that pair; night 11 counted `overflow-x`
the same way. `.floater`'s `position: absolute` is **just as much an echo of its
own declaration text** as `.pre`'s `white-space: pre` — the only thing that made
it more was the losing `div { position: static }`. Night 12 applied a stricter
test to this one property than the loop had applied to three others.

So the inconsistency had to be resolved in one direction or the other. Resolving
it **downward** would mean retracting `box-sizing`, `position` and `overflow-x`
and reporting 7 of 12. I resolved it **upward**, because (b) here is *stronger*
than either prior keyword had: the consequence is read out of the **display
list** — an export path independent of the cascade the value came from — rather
than out of geometry. Flagging it as a decision rather than banking it silently.

### The assertion that proves it

The fixture's trailing rule gains one declaration, the same vehicle nights 9 and
11 used:

```css
.pre { white-space: pre }
div  { box-sizing: content-box; position: static; white-space: normal }  /* LAST */
```

```python
assert pre["computed"]["white-space"] == "pre", pre["computed"]
# (a) the cascade decided it, and the rule it beat is named rather than dropped.
# Echoing the final matching declaration would say `normal` here.
pre_ws = next(x for x in pre["declared"] if x["property"] == "white-space")
assert pre_ws["winner"]["selector"] == ".pre", pre_ws["winner"]
assert pre_ws["winner"]["specificity"] == [0, 1, 0], pre_ws["winner"]
assert pre_ws["origin"] == "author", pre_ws
assert len(pre_ws["overridden"]) == 1, pre_ws["overridden"]
pre_loser = pre_ws["overridden"][0]
assert pre_loser["selector"] == "div", pre_loser
assert pre_loser["specificity"] == [0, 0, 1], pre_loser
assert pre_loser["value"] == "normal", pre_loser
# (b) and layout/paint really used `pre`: the div's own text keeps BOTH spaces,
# against a control on the same page in the same run that collapses to one.
assert "a  b" in texts and "a b" not in texts, sorted(texts)
assert len(texts["a  b"]["advances"]) == 4, texts["a  b"]
assert len(texts["c d"]["advances"]) == 3, texts["c d"]   # collapsed control
```

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
ok  computed expansion padding-left=16px, cited to `padding: 16px` on .hero (via_shorthand) — no rule spells the longhand
ok  origin split      h1 font-weight=700 winner=None (UA, no rule to cite); font-size=32px winner=h1 (author)
ok  line-height       .copy 20px x 1.5 = 30px — authored '1.5', computed 30px (resolved, not echoed)
ok  normal not faked  .hero line-height=normal (keyword, not px — resolving it needs font metrics)
ok  inherited origin  span font-family='Georgia, serif' text-align=center both origin=inherited; display still UA-or-initial
ok  line-height inherited  span declares nothing, reports 30px origin=inherited (.copy 20px x 1.5) and its line box IS 30.0 — `normal` control measures 20.0
ok  font-style        span computed=italic origin=inherited (declares nothing); paint drew it font_style=1, h1 still 0
ok  letter-spacing    .spaced 0.1em x 20px = 2px (authored '0.1em'); every advance is exactly +2.0 over .plain — [2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0]
ok  white-space      .pre computed=pre won by .pre [0, 1, 0] over a LATER div [0, 0, 1] normal; paint kept both spaces — 'a  b' has 4 advances against 3 for the collapsed control
ok  KNOWN DIVERGENCE  .pre keeps 'a  b' (4 advances) but its nested span collapsed 'c  d' to 'c d' — white-space is not inherited onto elements
ok  KNOWN GAP        a BARE <pre> computes white-space=normal (winner=None) and collapses 'x  y' to 'x y' — the UA sheet reached it (display=block, monospace) but has no white-space declaration to give
ok  border shorthand  .framed border-top-width=5px (0.25em x 20px) and border-top-color=rgba(204, 102, 0, 1), both cited to `border` on .framed; layout reserved 5.0 and paint drew a 210.0x5.0 band
ok  no false citation `border: 2px solid` cites the width and NOT border-top-color (the declaration carried no colour)
ok  longhand wins     .overruled border-top-width=9px beats `border: 3px solid #093`, which is reported in overridden; layout reserved {'bottom': 3.0, 'left': 3.0, 'right': 3.0, 'top': 9.0} and a 52.0-tall box
ok  box-sizing        .bordered border-box: declared 200px IS the border box, content 170.0x10.0; .content content-box, same declarations, border box 230.0x70.0
ok  position          .floater absolute (beat `div{position:static}`, last in sheet); .host is 0.0 tall around a 40.0-tall child and the next block starts at the same y — out of flow
ok  overflow-x       .clipped overflow-x=hidden cited to `overflow`; it establishes a BFC so the kid's 20px bottom margin lands INSIDE — 35.0 tall against 15.0 for .unclipped, same child, one keyword apart
ok  KNOWN GAP        ...but paint pushes no clip for it — overflow-x is answered for what LAYOUT did, not for whether anything was clipped
ok  KNOWN GAP        .faded opacity=0.5 computed, but paint filled it at a=1.0 — opacity never reaches the solid-colour path, so it is reported, not counted
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

**Checked that the gate can go red**, and it goes red on *both* halves. Sorting
the cascade on source order only (specificity ignored) in
`compute_style_for_element`:

```
computed:         normal          (was pre)
winner selector:  div  value: normal   (was .pre / pre)
PAINT texts:      {'Attribution': 11, 'inherited': 9, 'spacing': 7,
                   'a b': 3, 'c d': 3, 'x y': 3}     ← 'a  b'/4 is gone
```

All three of the value, the provenance and the paint consequence fail, so the
assertion tracks the engine and not the call. Perturbation reverted;
`grep -c RED-CHECK crates/rustkit-engine/src/lib.rs` returns 0 and `git status`
shows the crate clean.

### What the engine still cannot answer

- **`opacity` — the last one, and it is blocked exactly where night 12 left
  it.** I re-checked it against the standard above rather than inheriting the
  verdict, and it fails the **(b)** half outright: there is no consequence that
  differs as a function of the value. The cascade computes and clamps it (so
  (a)-style evidence is available), `DisplayCommand::Image.opacity` is populated
  from `layout_box.style.opacity` and exported — and the renderer's own Image arm
  then drops it (`Image { opacity: _, .. } => self.draw_image(url, *dest_rect)`),
  the solid-colour path never sees it (`.faded` fills at a=1.0), and it never
  reaches `rustkit-compositor` at all, so no layer is created. An assertion could
  only prove the cascade agrees with itself. **This one needs a rendering change;
  it cannot be moved inside this loop's scope.**
- **`white-space` is counted, and its two engine gaps are unchanged** — do not
  read the count as "white-space works". It is answerable for **author-declared**
  cases. An unstyled page still gets the wrong answer twice over: no element
  inheritance (a `<span>` inside a `pre` computes and lays out `normal`), and no
  UA default (a bare `<pre>` collapses its whitespace, because the UA arm sets
  display, font-family and margins and says `// white-space: pre (not
  implemented)` in the source). Both remain pinned by tripwires that flip when
  fixed; night 12 verified the second is a one-line fix.
- Everything nights 1–12 named is still true and is not repeated in full here:
  UA-origin properties have no rule to cite and cannot be distinguished from
  "nobody set it"; computed values cover a fixed longhand set; `hiwave_style`
  takes simple selectors only; `overflow-x` is answered for what layout did and
  **not** for whether anything was clipped; the display list's unmodelled ops
  (form controls, carets, focus rings, backdrop filters, gradient text, SVG
  primitives) carry `"modelled": false` with no contract and no coverage;
  capture-kind references are refused rather than implemented; `style` is not a
  diffable stage; and the diff corpus is two cases, so no real page is covered.
- **`!important` is still dead in the cascade** (night 2's finding), still pinned
  by the `important-width` diff case.

### Tests

**No Rust changed tonight** — the committed diff is `crates/hiwave-mcp/smoke.py`
only (+59 / −16). `cargo test -p rustkit-engine`, the crate the new assertion
describes and the one the red-check perturbed: **36 passed, 0 failed**. I did not
re-run the full workspace, because no compiled input to it changed; nights 7–12's
standing result (914 passed, 1 pre-existing
`rustkit-layout::probe_normal_line_height_vs_chrome` failure) is unaffected by a
test-script edit. The perturbation was reverted and verified reverted **before**
the commit.

Scope stayed inside `crates/hiwave-mcp/`. No parity harness, no `.github/`, no
Windows or Linux port work, no engine behaviour changed, and no export that
already had passing assertions had its answers altered. The runner needed
`mesa-vulkan-drivers` installed again (fresh container; environment only, nothing
committed — and note `apt-get update` now fails on two unrelated PPAs
(`deadsnakes`, `ondrej/php`: "no longer signed" / 403), so install the package
**without** an update first, which works).

### Decisions needed from Pete

1. **Confirm or reject tonight's ruling, because it sets the count.** If the
   keyword standard is (a)+(b) as night 9 wrote it, `white-space` counts and the
   metric is **11 of 12**. If instead a keyword needs a shorthand, inheritance or
   a UA default behind it — night 12's stricter test — then `white-space` does
   not count *and neither do `box-sizing`, `position` and `overflow-x`*, and the
   honest metric is **7 of 12**. What is not defensible is the mixture the digest
   held until tonight. I have applied the first reading; say if you want the
   second and I will retract the three and restate the number.
2. **The remaining work is all rendering changes, which `BASELINE.md` forbids
   this loop.** `opacity` needs a consumer that honours it (renderer Image arm,
   solid-colour path, or compositor layer); the bare-`<pre>` UA gap — carried
   unanswered from night 12 — needs one line in the `"pre"` arm. So 12 of 12 is
   not reachable inside the current scope. Three ways out: authorise the renderer
   change here, hand both to the parity trench, or **close trench 2 at 11 of 12**
   and point the next loop at a new metric. My read: close at 11 and hand the two
   rendering bugs over, since both are parity-visible and this loop is the wrong
   owner for them.
3. **The stored prompt is stale for the eighth night.** It still describes trench
   1 and tells the session to stop at 4 of 4. `BASELINE.md`'s warning block is
   what saved the night again — the scheduler's checkout starts detached at
   master, where the digest ends at night 3 and agrees with the prompt. It cost a
   discarded commit on night 12 and it will keep costing until the branch lands
   or the prompt is repointed.

**Next night, first move:** `git ls-remote origin 'refs/heads/atlas/*'` and check
out `atlas/trench-mcp-exports` BEFORE reading `trench/`, then rebuild
(`cargo build -p hiwave-mcp`) — a stale `target/` from master silently fails
night 8's provenance assertions and looks like a regression. Then: unless a
rendering change is authorised per decision 2, there is no property left that
this loop may move, and the correct action is the funeral note at 11 of 12.

---

## 2026-08-11 — night 14 (NONE — and the funeral note for trench 2)

**Metric: 11 of 12 → 11 of 12**

**Moved no → yes: NONE.**

Not "partial progress". Nothing moved, and nothing *could* move: the one
remaining property is blocked by a scope rule rather than by difficulty, so a
further night cannot produce a different answer. The funeral is below.

### What I ran, so the state is checkable rather than inherited

The number above is not carried over from night 13. Cold runner, cold
`target/`, fresh build, full smoke run on this branch tonight — all 11 counted
properties re-asserted and passing:

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
ok  computed expansion padding-left=16px, cited to `padding: 16px` on .hero (via_shorthand) — no rule spells the longhand
ok  origin split      h1 font-weight=700 winner=None (UA, no rule to cite); font-size=32px winner=h1 (author)
ok  line-height       .copy 20px x 1.5 = 30px — authored '1.5', computed 30px (resolved, not echoed)
ok  normal not faked  .hero line-height=normal (keyword, not px — resolving it needs font metrics)
ok  inherited origin  span font-family='Georgia, serif' text-align=center both origin=inherited; display still UA-or-initial
ok  line-height inherited  span declares nothing, reports 30px origin=inherited (.copy 20px x 1.5) and its line box IS 30.0 — `normal` control measures 20.0
ok  font-style        span computed=italic origin=inherited (declares nothing); paint drew it font_style=1, h1 still 0
ok  letter-spacing    .spaced 0.1em x 20px = 2px (authored '0.1em'); every advance is exactly +2.0 over .plain — [2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0]
ok  white-space      .pre computed=pre won by .pre [0, 1, 0] over a LATER div [0, 0, 1] normal; paint kept both spaces — 'a  b' has 4 advances against 3 for the collapsed control
ok  KNOWN DIVERGENCE  .pre keeps 'a  b' (4 advances) but its nested span collapsed 'c  d' to 'c d' — white-space is not inherited onto elements
ok  KNOWN GAP        a BARE <pre> computes white-space=normal (winner=None) and collapses 'x  y' to 'x y' — the UA sheet reached it (display=block, monospace) but has no white-space declaration to give
ok  border shorthand  .framed border-top-width=5px (0.25em x 20px) and border-top-color=rgba(204, 102, 0, 1), both cited to `border` on .framed; layout reserved 5.0 and paint drew a 210.0x5.0 band
ok  no false citation `border: 2px solid` cites the width and NOT border-top-color (the declaration carried no colour)
ok  longhand wins     .overruled border-top-width=9px beats `border: 3px solid #093`, which is reported in overridden; layout reserved {'bottom': 3.0, 'left': 3.0, 'right': 3.0, 'top': 9.0} and a 52.0-tall box
ok  box-sizing        .bordered border-box: declared 200px IS the border box, content 170.0x10.0; .content content-box, same declarations, border box 230.0x70.0
ok  position          .floater absolute (beat `div{position:static}`, last in sheet); .host is 0.0 tall around a 40.0-tall child and the next block starts at the same y — out of flow
ok  overflow-x       .clipped overflow-x=hidden cited to `overflow`; it establishes a BFC so the kid's 20px bottom margin lands INSIDE — 35.0 tall against 15.0 for .unclipped, same child, one keyword apart
ok  KNOWN GAP        ...but paint pushes no clip for it — overflow-x is answered for what LAYOUT did, not for whether anything was clipped
ok  KNOWN GAP        .faded opacity=0.5 computed, but paint filled it at a=1.0 — opacity never reaches the solid-colour path, so it is reported, not counted
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

### `opacity` re-checked from source, not inherited from night 13

Nights 12 and 13 both declared `opacity` blocked. I did not take either on
trust, because "the previous night said so" is how a stalled loop starts
reporting motion. Traced it end to end tonight, and the verdict holds — with
one detail sharper than the digest has recorded so far.

- **The cascade computes it properly.** Parsed and clamped to `[0,1]`
  (`rustkit-engine/src/lib.rs:3628`), so clause 2 of the standard is
  satisfiable on its own: authored `1.5` reports `1`, which is a computed value
  and not an echo.
- **Layout threads it into exactly one command.** `layout_box.style.opacity` is
  passed only to the image command (`rustkit-layout/src/lib.rs:4925`).
- **The renderer then discards it there.** The `Image` arm destructures it away
  — `opacity: _` — and calls `draw_image(url, *dest_rect)`
  (`rustkit-renderer/src/lib.rs:1895`).
- **For solid colours it is not discarded — it was never carried.** This is the
  detail worth pinning: `DisplayCommand::SolidColor(Color, Rect)`
  (`rustkit-layout/src/lib.rs:3401`) is a two-field tuple variant with **no
  opacity field at all**. So `.faded` filling at `a=1.0` is not a renderer
  oversight that a one-line read would fix; the value has no channel to travel
  down. Making it observable means changing the display-list *shape*, not
  reading an existing field.
- **The compositor never sees it.** `grep -c opacity crates/rustkit-compositor/src/lib.rs`
  returns **0** — no layer is created, so there is no compositing consequence
  either.
- The only other alpha site in the renderer (`lib.rs:4517`, `color.a` on a
  colour-glyph run) is the text run's own colour alpha. It is not element
  opacity and is not fed from `style.opacity`.

So there is no consequence anywhere in the engine that differs as a function of
the value. An assertion could only show the cascade agreeing with itself, which
fails clause 3 outright — and clause 3 is explicit that a property whose
reported value can differ from the value the engine used **does not count at
all**. `BASELINE.md`'s trench 2 scope limit forbids the fix: *"Do not fix the
engine to make a property answerable... Rendering changes belong to the parity
corpus, not to an export loop."* No authorisation to lift it has arrived, so
the property stays uncounted and the tripwire stays as the honest answer.

### FUNERAL — trench 2 stops at 11 of 12

Writing this now rather than after one more firing, and being explicit about
why, because the trigger matters as much as the conclusion.

**The literal stop condition has not fired.** Clause 2 is two *consecutive*
dry nights; night 12 was NONE, night 13 moved `white-space`, tonight is NONE —
that is one, not two. I am not claiming otherwise, and I am not restating the
rule to fit the outcome.

What has happened instead is that the remaining work became **provably
out of scope** rather than merely hard. Clause 2 exists to catch a loop that
cannot tell stalling from working. Here the loop can tell, precisely: eleven
properties are answered, the twelfth needs a rendering change that this trench
is forbidden to make, and no further night of the same effort can change that
— the blocker is a rule, and rules do not yield to another night's work. A
fourteenth-night funeral and a fifteenth-night funeral would contain the same
text; the only difference is one wasted run.

**Where it stopped:** 0 of 12 (2026-08-03) → **11 of 12** (2026-08-10), across
nights 5–13. Counted: `line-height`, `font-family`, `text-align`, `font-style`,
`letter-spacing`, `white-space`, `border-top-width`, `border-top-color`,
`box-sizing`, `position`, `overflow-x`. Uncounted: `opacity`.

**The specific technical reason the last one did not become answerable:**
`opacity` has no consumer. The cascade computes and clamps it; layout forwards
it to the image command only; the renderer drops it there; the solid-colour
display command has no field to carry it; the compositor has no reference to it
at all. Nothing the engine produces differs between `opacity: 0.5` and
`opacity: 1`, so no assertion can prove the engine used the value — only that
it stored it.

**What would have to change for the work to be worth resuming.** One of:

1. **Authorise the rendering change.** Three sites, and now scoped precisely:
   give `DisplayCommand::SolidColor` an alpha channel (or wrap painted subtrees
   in a layer), honour it in the renderer's `Image` arm, and let the compositor
   see it. Then `opacity` becomes answerable on the existing standard in a
   single night — the fixture and the tripwire are already committed. This is a
   parity-visible rendering change, which is exactly why it was fenced off.
2. **Point the next loop at a new metric.** 11 of 12 is a real stopping place,
   not a failure: the diagnosis set was chosen because parity blames text, and
   the entire text group is answered.
3. **Nothing.** The tool is honest about the gap today — `limits` in the payload
   says `opacity` "says nothing about what was drawn", and the smoke test pins
   it as a KNOWN GAP that flips the day a consumer appears. Leaving it is a
   defensible end state.

My read, unchanged from night 13's: **close at 11 of 12** and hand the two
rendering bugs (`opacity`, and the one-line bare-`<pre>` UA gap) to the parity
trench, which owns rendering changes and has the corpus to see their effect.

### What the engine still cannot answer

- **`opacity`** — as above. The last property in the diagnosis set, and the only
  one this loop was never permitted to move.
- **`white-space` is counted but has two live engine gaps**, both still pinned
  by tripwires and both re-verified passing tonight: it does not inherit onto
  elements (a `<span>` inside a `pre` computes and lays out `normal`), and a
  bare `<pre>` gets no UA `white-space` and collapses its spaces. Night 12
  verified the second is a one-line fix. Do not read "counted" as "works".
- **`overflow-x` is answered for what layout did, not for whether anything was
  clipped** — paint pushes no clip for it.
- **`!important` is still dead in the cascade** (night 2's finding), still
  pinned by the `important-width` diff case.
- Everything nights 1–13 named remains true and is not restated in full: UA
  origin cannot be distinguished from "nobody set it"; computed values cover a
  fixed longhand set; `hiwave_style` takes simple selectors only; the display
  list's unmodelled ops carry `"modelled": false` with no contract and no
  coverage; capture-kind references are refused rather than implemented; `style`
  is not a diffable stage; and the diff corpus is two fixtures, so **no real
  page is covered by any of the four tools**.

### Tests

**No Rust changed tonight** — the committed diff is `trench/digest.md` only, so
nothing compiled can have regressed. Ran `cargo test -p rustkit-engine` anyway,
as the crate every assertion above describes: **36 passed, 0 failed**. The full
smoke suite passed as pasted. Nights 7–13's standing workspace result (914
passed, 1 pre-existing `rustkit-layout::probe_normal_line_height_vs_chrome`
failure) is untouched by a markdown edit.

Scope stayed inside `trench/`. No parity harness, no `.github/`, no Windows or
Linux port work, no engine behaviour changed, and no export that already had
passing assertions was altered.

Runner notes for whoever picks this up: `target/` was **empty**, so budget a
cold build (~2 min for `-p hiwave-mcp`). `mesa-vulkan-drivers` was missing again
and the cached apt index was stale — `apt-get install` alone 404s, so run
`apt-get update` first and ignore the two PPA errors (`deadsnakes`,
`ondrej/php`), which do not block the main archive. That is the reverse of
night 13's advice, which no longer works.

### Decisions needed from Pete

1. **Authorise the renderer change for `opacity`, or close trench 2 at 11 of
   12?** This is the only thing standing between the metric and 12 of 12, and
   the loop cannot answer it itself. Scoped above to three sites; roughly one
   night's work if authorised. My read: close at 11 and hand it to the parity
   trench.
2. **Night 13's ruling still needs confirming, because it sets the number.** If
   the keyword standard is (a)+(b) as night 9 wrote it, the metric is **11 of
   12** as reported here. If a keyword instead needs a shorthand, inheritance or
   a UA default behind it, then `white-space`, `box-sizing`, `position` and
   `overflow-x` all fall and the honest number is **7 of 12**. I have carried
   night 13's reading forward. Say the word and I will restate it.
3. **The stored prompt is stale for the ninth night**, and it now costs more
   than time: it describes trench 1 and instructs the session to stop at 4 of 4,
   which is a stop condition that was met nine days ago. `BASELINE.md`'s warning
   block caught it again tonight — the scheduler's checkout starts at master,
   where the digest ends at night 3 and corroborates the stale prompt, so an
   agent that trusts its instructions over the repo files closes the wrong
   trench. Repointing the routine or landing this branch to master both fix it.
