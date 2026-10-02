## What

The `background` shorthand never cleared a colour. It set `background-color` only when its whole value was a colour, so every other form left an earlier colour painted: `background: none`, `background: 0 0` (what minifiers write for `none`), an image alone, a gradient alone, `initial`, `unset`. Chrome resets every longhand the shorthand does not name (css-backgrounds-3 §3.10). The reverse was wrong too: a colour beside an image (`background: #fff url(a.png) no-repeat`) was dropped, so the earlier colour stayed where the new one belonged.

This is what kept #429 (the push button's default box and face) in draft: once a button's grey face arrives through the cascade, a page that removes it with `background: none` kept it (weather.com's "More" button).

A fixture on the hub branch (`scratch/s1002b/bgsh.html`): eighteen boxes, each given a colour (or a gradient) by an earlier rule and then one later declaration. The colour painted at each box's centre, pinned Chrome 148 against both arms (`scratch/s1002b/bgsh_cmp.py`):

```
later declaration, over `background-color: red`        Chrome 148   develop __BASE__   fix
background: none                                       clear        red                clear
background: 0 0                                        clear        red                clear
background: unset                                      clear        red                clear
background: initial                                    clear        red                clear
background: url(nope.png) no-repeat                    clear        red                clear
background: linear-gradient(transparent, transparent)  clear        red                clear
background: green                                      green        green              green
background: green url(nope.png) no-repeat              green        red                green
background: url(nope.png) no-repeat green              green        red                green
background-image: none                                 red          red                red
background: -moz-linear-gradient(top, blue, blue)      red          red                red
background: transparent                                clear        clear              clear
background: inherit (parent is white)                  white        red                red
background: none; background-color: green              green        green              green
background: none !important, then background-color     clear        green              clear
over `background: linear-gradient(blue, blue)`
background: none                                       clear        blue               clear
background: -moz-linear-gradient(top, red, red)        blue         blue               blue

boxes whose colour differs from Chrome                 -            11 of 18           1 of 18
```

The one left is `background: inherit`: the engine treats `inherit` on a property that does not inherit as "leave it", for every such property. Not changed here.

## Change

`crates/rustkit-engine/src/lib.rs`, the `"background" | "background-image"` arm of `apply_style_property` and `apply_initial_value`:

- The shorthand starts from a transparent colour, then takes the colour its value names: the whole value, or one top-level token of a layer (`#fff url(a.png) no-repeat`, either order).
- `background: initial` and `background: unset` clear the colour and the image layers (`apply_initial_value` had no arm for the shorthand).
- The legacy `background_gradient` field is cleared with the layers. Paint falls back to it, so `background: none` over a gradient kept the gradient.
- A value in another engine's prefix (`-moz-`, `-o-`, `-ms-`) is dropped whole, as Chrome drops a declaration it cannot parse. Before, it cleared the earlier image layers; with the reset it would have cleared the colour too.
- `background-image` is a longhand and still leaves the colour alone.

Not in this PR (each measured on the fixtures, none new):

- `background: inherit` (above).
- The shorthand's position, size, repeat and boxes are not read: `background: url(a.png) no-repeat center / cover` keeps only the image, and only when the layer begins with it. That is the next PR, stacked on this one.
- `all: unset` is not applied, and a four-digit hex colour (`#0000`) does not parse.

## Tests

One engine test, `the_background_shorthand_resets_the_colour_it_does_not_name`: the five resets over an earlier colour; a colour alone, before an image and after one; `background-image: none` leaving the colour; a foreign-prefixed value leaving colour and image; `none` over a gradient removing the layer and the legacy gradient.

Fail-first: the test spliced into develop's test module, non-test code untouched (`scratch/s1002b/failfirst_bg.py`):

```
__FAILFIRST__
```

__SUITES__

## Receipt

Both arms were release-built from clean sources against the same target directory and run on the develop worktree's fixtures (`scratch/s1001c/camp3.py`): develop `__BASE__`, fix `__HEAD__`.

```
__SUMMARY__
```

__RATCHET__

<details><summary>Per-case diff_pct, scope all (develop | fix)</summary>

| case | develop | fix |
|---|---|---|
__TABLE_ALL__

</details>

<details><summary>Per-case diff_pct, scopes builtins and micro (develop | fix)</summary>

| case | develop | fix |
|---|---|---|
__TABLE_REST__

</details>

## Real sites

RustKit frames only, each site captured develop, fix, develop, fix at 1280x800 (`scratch/s1001g/ab.py`). "within" is the difference between two captures of the same binary (the site's own variance); "across" is develop against fix, all four pairs.

```
__FRAMES__
```

__MOVERS__

🤖 Generated with [Claude Code](https://claude.com/claude-code)
