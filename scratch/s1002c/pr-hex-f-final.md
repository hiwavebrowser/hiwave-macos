## What

`#rgba` is the short form of `#rrggbbaa` (css-color-4 §5.2): each digit is doubled. `rustkit_css::parse_color` knew three, six and eight digits, so a declaration with a four-digit colour was dropped and whatever was under it stayed.

It matters because minifiers write `transparent` as `#0000`:

- Tailwind's preflight: `button,input,select,optgroup,textarea { background-color:#0000 }` (weather.com).
- squarespace: `.cta--inline { appearance:none; background-color:#0000 }` on its navigation buttons.
- github: its header and hero (see Real sites).

Found while taking #429 (the push button's default box) out of draft: with a button's face coming through the cascade, these two resets are what should remove it, and they were dropped.

A fixture on the hub branch (`scratch/s1002c/hex4.html`, thirteen 120 x 40 boxes over red). The colour painted at each box's centre, pinned Chrome 148 against each binary (`scratch/s1002b/bgsh_cmp.py`):

```
d0   chrome (255, 0, 0)      computed rgb(255, 0, 0)      develop (255, 0, 0)       fix (255, 0, 0)       |  
d1   chrome (255, 255, 255)  computed rgba(0, 0, 0, 0)    develop (255, 0, 0)      WRONG  fix (255, 255, 255)   |  #d1 { background-color: #0000; }
d2   chrome (255, 255, 255)  computed rgba(0, 0, 0, 0)    develop (255, 255, 255)   fix (255, 255, 255)   |  #d2 { background: #0000; }
d3   chrome (119, 119, 255)  computed rgba(0, 0, 255, 0.533)  develop (255, 0, 0)      WRONG  fix (119, 119, 255)   |  #d3 { background-color: #00f8; }
d4   chrome (0, 255, 0)      computed rgb(0, 255, 0)      develop (255, 0, 0)      WRONG  fix (0, 255, 0)       |  #d4 { background-color: #0F0F; }
d5   chrome (127, 255, 127)  computed rgba(0, 255, 0, 0.5)  develop (127, 255, 127)   fix (127, 255, 127)   |  #d5 { background-color: #00ff0080; }
d6   chrome (255, 0, 0)      computed rgb(255, 0, 0)      develop (255, 0, 0)       fix (255, 0, 0)       |  #d6 { background-color: #12345; }
d7   chrome (255, 0, 0)      computed rgb(255, 0, 0)      develop (255, 0, 0)       fix (255, 0, 0)       |  #d7 { background-color: #ggg; }
d8   chrome (119, 119, 255)  computed rgba(0, 0, 0, 0)    develop (255, 255, 255)  WRONG  fix (119, 119, 255)   |  #d8 { background: #0000 linear-gradient(#00f8, #00f8); }
d9   chrome (255, 255, 255)  computed rgba(255, 255, 255, 0)  develop (255, 0, 0)      WRONG  fix (255, 255, 255)   |  #d9 { border: 10px solid #0808; background-color: #fff0; }
d10  chrome (185, 119, 188)  computed rgba(0, 0, 0, 0)    develop (255, 255, 255)  WRONG  fix (185, 119, 189)   |  #d10 { background: linear-gradient(#f008, #00f8); }
d11  chrome (170, 187, 204)  computed rgb(170, 187, 204)  develop (170, 187, 204)   fix (170, 187, 204)   |  #d11 { background-color: #abc; }
d12  chrome (255, 0, 0)      computed rgb(255, 0, 0)      develop (15, 15, 15)     WRONG  fix (255, 0, 0)       |  #d12 { background-color: #+f+f+f; }
boxes whose painted colour differs from Chrome: develop 7 of 13, fix 0 of 13
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
---- element_identity_tests::a_four_digit_hex_colour_replaces_an_earlier_one stdout ----

thread 'element_identity_tests::a_four_digit_hex_colour_replaces_an_earlier_one' (17025300) panicked at crates/rustkit-engine/src/lib.rs:16599:9:
assertion `left == right` failed: `background-color: #0000`
  left: Color { r: 255, g: 0, b: 0, a: 1.0 }
 right: Color { r: 0, g: 0, b: 0, a: 0.0 }
```

Suites on the branch: `cargo test -p rustkit-css --lib` 49/49; `cargo test -p rustkit-engine --lib background` 8/8 and the new engine test 1/1. The full engine suite did not run locally (the machine was at load 11 to 18); CI's `unit-suites` runs it.

## Receipt

Both arms were release-built from clean sources against the same target directory and run on the develop worktree's fixtures (`scratch/s1001c/camp3.py`). develop is `97393a7`; fix is `71bfac0`.

```
all:      develop 2026-10-02T07:45:24 26/26 avg 1.1069 | fix 2026-10-02T07:54:26 26/26 avg 1.1069 | 26/26 identical
builtins: develop 2026-10-02T07:41:35  5/5  avg 1.8139 | fix 2026-10-02T07:50:59  5/5  avg 1.8139 | 5/5 identical
micro:    develop 2026-10-02T07:40:50 13/13 avg 0.6548 | fix 2026-10-02T07:50:18 13/13 avg 0.6548 | 13/13 identical
```

`ratchet_local.py` (CI's Gate A / Gate B / ratchet): output identical on both arms, line for line.

<details><summary>Per-case diff_pct, scope all (develop | fix)</summary>

| case | develop | fix |
|---|---|---|
| about | 3.6742 | 3.6742 |
| article-typography | 4.7857 | 4.7857 |
| backgrounds | 1.2163 | 1.2163 |
| bg-pure | 0.0000 | 0.0000 |
| bg-solid | 0.2106 | 0.2106 |
| card-grid | 1.3034 | 1.3034 |
| chrome_rustkit | 1.1234 | 1.1234 |
| combinators | 0.6547 | 0.6547 |
| css-selectors | 1.3299 | 1.3299 |
| flex-positioning | 0.6327 | 0.6327 |
| form-controls | 3.2388 | 3.2388 |
| form-elements | 0.9535 | 0.9535 |
| gpu-gradient-regression | 0.3903 | 0.3903 |
| gradient-backgrounds | 1.0133 | 1.0133 |
| gradient-no-radius | 0.5204 | 0.5204 |
| gradient-radius-only | 0.3429 | 0.3429 |
| gradients | 0.1412 | 0.1412 |
| image-gallery | 0.5217 | 0.5217 |
| images-intrinsic | 0.3267 | 0.3267 |
| new_tab | 1.1019 | 1.1019 |
| pseudo-classes | 0.4341 | 0.4341 |
| rounded-corners | 0.4354 | 0.4354 |
| settings | 2.0919 | 2.0919 |
| shelf | 1.0781 | 1.0781 |
| specificity | 0.6015 | 0.6015 |
| sticky-scroll | 0.6575 | 0.6575 |

</details>

<details><summary>Per-case diff_pct, scopes builtins and micro (develop | fix)</summary>

| case | develop | fix |
|---|---|---|
| about | 3.6742 | 3.6742 |
| chrome_rustkit | 1.1234 | 1.1234 |
| new_tab | 1.1019 | 1.1019 |
| settings | 2.0919 | 2.0919 |
| shelf | 1.0781 | 1.0781 |
| backgrounds | 1.2163 | 1.2163 |
| bg-pure | 0.0000 | 0.0000 |
| bg-solid | 0.2106 | 0.2106 |
| combinators | 0.6547 | 0.6547 |
| form-controls | 3.2388 | 3.2388 |
| gpu-gradient-regression | 0.3903 | 0.3903 |
| gradient-no-radius | 0.5204 | 0.5204 |
| gradient-radius-only | 0.3429 | 0.3429 |
| gradients | 0.1412 | 0.1412 |
| images-intrinsic | 0.3267 | 0.3267 |
| pseudo-classes | 0.4341 | 0.4341 |
| rounded-corners | 0.4354 | 0.4354 |
| specificity | 0.6015 | 0.6015 |

</details>

## Real sites

RustKit frames only, each site captured develop, fix, develop, fix at 1280x800 (`scratch/s1001g/ab.py`). "within" is the difference between two captures of the same binary (the site's own variance); "across" is one arm against the other, all four pairs; `nan` is a capture that did not finish in 30 s.

```
weather      within develop   0.00%  within fix   0.00%  across   0.01%   0.01%   0.01%   0.01%  failed: none  secs A,B,A,B 11,11,17,11
squarespace  within develop    nan%  within fix    nan%  across    nan%    nan%    nan%    nan%  failed: A1,B1,A2,B2  secs A,B,A,B 30,30,30,30
walmart      within develop  34.82%  within fix   0.00%  across   0.00%   0.00%  34.82%  34.82%  failed: none  secs A,B,A,B 13,10,11,14
google       within develop   8.49%  within fix   0.03%  across   8.49%   8.52%   0.00%   0.03%  failed: none  secs A,B,A,B 12,14,15,13
facebook     within develop    nan%  within fix    nan%  across    nan%    nan%    nan%    nan%  failed: A1,B1,A2  secs A,B,A,B 31,30,30,27
wikipedia    within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 14,13,11,11
lyft         within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 16,20,19,18
x            within develop    nan%  within fix   0.00%  across   0.00%   0.00%    nan%    nan%  failed: A2  secs A,B,A,B 10,10,7,13
linkedin     within develop   2.96%  within fix   2.64%  across   2.64%   0.00%   1.55%   2.96%  failed: none  secs A,B,A,B 10,14,12,14
yahoo        within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 26,22,22,22
bing         within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 8,5,7,7
apple        within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 23,23,23,21
netflix      within develop   4.86%  within fix  12.51%  across  12.53%   2.66%  12.70%   4.81%  failed: none  secs A,B,A,B 23,23,26,24
shopify      within develop   0.00%  within fix   0.00%  across   0.05%   0.05%   0.05%   0.05%  failed: none  secs A,B,A,B 23,27,23,24
github       within develop    nan%  within fix   0.00%  across    nan%    nan%  97.72%  97.72%  failed: A1  secs A,B,A,B 30,24,23,24
cnn          within develop    nan%  within fix    nan%  across    nan%    nan%    nan%    nan%  failed: A1,B1,A2,B2  secs A,B,A,B 32,32,11,32
youtube      within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 19,14,12,13
instagram    within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 27,29,29,27
reddit       within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 5,5,5,5
microsoft    within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 12,12,11,12
```

What moved, and what I looked at:

- **github: the whole frame.** On develop the header is a black band, the hero sits on a flat panel and the e-mail field is filled white; on the fix the header is clear over the hero's gradient and the field is clear. Scored with the oracle's own diff against the Chrome frames stored by the 2026-10-01 14:54 quiet board (`scratch/s1001g/vs_chrome.py`; not a same-run Chrome capture, so read it as a size, not a board score): **develop 75.06%, fix 16.24%**. Three of four captures finished (one develop capture hit the 30 s limit); the two fix captures are identical to each other.
- **weather 0.01%** (125 pixels in the location pill's label) and **shopify 0.05%** (536 pixels at a button's edge): against the same stored Chrome frames weather is 30.53 -> 30.54% and shopify 19.906 -> 19.905%.
- **netflix, google, walmart, linkedin:** each serves more than one page (netflix alternates two headlines and button styles), and the difference within one arm is as large as across arms. I looked at netflix's two fix captures: two different pages.
- **Not verified: squarespace, facebook, cnn.** None finished inside the 30 s limit on either arm in two attempts (machine load 11 to 14). squarespace is one of the two sites this was found on (`.cta--inline { background-color:#0000 }`), so it should move.
- The other ten sites are pixel-identical across every capture that finished (x lost one develop capture to the 30 s limit; the other nine have all four).

No board check is claimed: no scoring run was taken.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
