## What

`#rgba` is the short form of `#rrggbbaa` (css-color-4 §5.2): each digit is doubled. `rustkit_css::parse_color` knew three, six and eight digits, so a declaration with a four-digit colour was dropped and whatever was under it stayed.

It matters because minifiers write `transparent` as `#0000`:

- Tailwind's preflight: `button,input,select,optgroup,textarea { background-color:#0000 }` (weather.com).
- squarespace: `.cta--inline { appearance:none; background-color:#0000 }` on its navigation buttons.
- github: its header and hero (see Real sites).

Found while taking #429 (the push button's default box) out of draft: with a button's face coming through the cascade, these two resets are what should remove it, and they were dropped.

A fixture on the hub branch (`scratch/s1002c/hex4.html`, thirteen 120 x 40 boxes over red). The colour painted at each box's centre, pinned Chrome 148 against each binary (`scratch/s1002b/bgsh_cmp.py`):

```
__FIXTURE__
```

## Change

`crates/rustkit-css/src/lib.rs`, `parse_color`:

- A four-digit arm: each digit times 17, the fourth is the alpha.
- A hex colour is checked to be hex digits only before it is read. `u8::from_str_radix` also takes a sign, so `#+f+f+f` was a colour (row d12 above).

The engine's `parse_color` delegates here, so colours inside gradients and the `background` shorthand follow.

## Tests

- rustkit-css: `a_four_digit_hex_colour_carries_its_alpha`, `a_hex_colour_is_hex_digits_only`.
- rustkit-engine: `a_four_digit_hex_colour_replaces_an_earlier_one` (`background-color: #0000` and `background: #0000` over red; `color: #00F8`).

Fail-first: the engine test on develop's code (the branch's engine `lib.rs`, which differs from develop only by the test, against develop's rustkit-css; `scratch/s1002c/failfirst_hex.py`):

```
__FAILFIRST__
```

__SUITES__

## Receipt

Both arms were release-built from clean sources against the same target directory and run on the develop worktree's fixtures (`scratch/s1001c/camp3.py`). develop is `__BASE__`; fix is `__HEAD__`.

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

RustKit frames only, each site captured develop, fix, develop, fix at 1280x800 (`scratch/s1001g/ab.py`). "within" is the difference between two captures of the same binary (the site's own variance); "across" is one arm against the other, all four pairs; `nan` is a capture that did not finish in 30 s.

```
__FRAMES__
```

__MOVERS__

🤖 Generated with [Claude Code](https://claude.com/claude-code)
