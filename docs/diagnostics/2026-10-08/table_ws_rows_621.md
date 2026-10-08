# #621 Whitespace between table rows: infobox reduced page

- **Issue:** [#621](https://github.com/hiwavebrowser/hiwave-macos/issues/621), filed by Prometheus during R1 on #619. When `<tr>`s are separated by whitespace, the parser gives each row its own `tbody`, and `</table>` never closes the table. Everything after it is parsed into the table.
- **Measured:** develop **`baac9807`** (Merge PR #617). Its only change since `6f6496cd`, where #621 was filed, is the real-site list (`websuite/realsite-top25.json`, `scripts/tests/test_holdout_board.py`). No engine code changed, so the RustKit binary is the `parity-capture --profile parity` build of `6f6496cd`, the same one used for #618.
- **Oracle:** pinned Chrome for Testing **148.0.7778.216**, headless, 800x600, DPR 1.
- **Pages** (Menlo 16px/20px; the infobox is `float: right; width: 260px`, 12px/16px):
  - [`table_ws_rows_infobox.html`](table_ws_rows_infobox.html) (15 lines): shaped like a Wikipedia infobox, `<table class="infobox"><tbody>`, then 4 `<tr>`s each on its own indented line, `</tbody></table>`, then `p#p1` and `p#p2`.
  - [`table_ws_rows_control.html`](table_ws_rows_control.html) (10 lines): the same markup with the four `<tr>`s back to back.

## Method

**Chromium.** `document.querySelector('table')`: the `tbody` count, `childNodes` (whitespace-only text shown as `WS`), each tbody's `childNodes`, and each `<p>`'s `parentElement`. Rects come from `getBoundingClientRect()`. Line 1 comes from `Range.selectNodeContents(p).getClientRects()` grouped by top, with the hanging trailing space left out.

**RustKit, box tree and layout.** `parity-capture --html-file <page> --width 800 --height 600 --dump-layout --dump-display-list`. The table's box children and each `<p>`'s box ancestors and `border_box` come from the layout dump. Line 1 is the first display-list `text` run inside the `<p>`'s box, with x and the sum of `advances`, trailing space trimmed.

**RustKit, DOM.** `--html-file` doesn't run scripts, so a scratch copy of each page (not committed) had a `<script>` added before `</body>`. It wrote `getElementsByTagName('tbody').length`, the table's `childNodes`, each tbody's `childNodes` and each `<p>`'s `parentNode` into a div. That copy was loaded with `--url` from a localhost server, and the text was read back from the display list. Its box tree is the same as the `--html-file` load.

## Results

| Page | Engine | tbody count (DOM) | table children (DOM) | p#p1 / p#p2 parent | table rect | p#p1 rect | p#p1 line 1 x / width | p#p1 text is |
|---|---|---|---|---|---|---|---|---|
| infobox (whitespace) | Chromium | 1 | `tbody` (tbody = `WS,tr,WS,tr,WS,tr,WS,tr,WS`) | body / body | 532,8 260x140 | 8,8 784x200 | 8 / 433.48 | **beside** the float (lines end before x=516) |
| infobox (whitespace) | RustKit | **5** (4 tbody boxes; the explicit `<tbody>` holds only whitespace and gets no box) | `tbody x5, WS, p, WS, p, WS, script` | **table / table** | 530,8 262x**638** | **531,129 260x272** | **531 / 252.86** | **inside** the table, under the 4 rows, in its 260px column |
| control (no whitespace) | Chromium | 1 | `tbody` (tbody = `tr,tr,tr,tr`) | body / body | 532,8 260x140 | 8,8 784x200 | 8 / 433.48 | beside the float |
| control (no whitespace) | RustKit | 1 | `tbody` (tbody = `tr,tr,tr,tr`) | body / body | 530,8 262x122 | 8,8 784x160 | 8 / 751.36 | **under** the float: full width and overlapping it (H19, #618) |
| #618 shape (b) `float_shape_b_infobox_table.html` | Chromium | 1 | `tbody` (implied) | body / body | 532,8 260x145 | 8,8 784x200 | 8 / 433.48 | beside the float |
| #618 shape (b) | RustKit | 1 (box tree: table > tbody > tr x4) | `tbody` | body / body | 530,8 262x122 | 8,8 784x160 | 8 / 751.36 | under the float (H19) |

So **#621 reproduces on develop**. On the infobox page, each whitespace-separated row opens a new tbody, and `</table>` doesn't pop. Both paragraphs and the trailing `<script>` become children of the `<table>`, and the body's only child is the table. In layout, the paragraphs stack under the rows inside the 262px float (table height 638 against Chromium's 140) and wrap at 260px. The control page, with no whitespace, parses like Chromium. There, the only difference left is H19, line boxes not shortened beside the float.

## #618 shape (b): not affected by #621

`float_shape_b_infobox_table.html` (on #618) writes the table on **one line with no whitespace between rows**: `<table class="infobox" id="float"><tr><td>…</td></tr><tr>…</tr></table>`, with no explicit `<tbody>`. In RustKit its box tree is `body > table > tbody > tr x4`, with `p#p1` and `p#p2` as children of `body`, like the control page. No tbodies are split and no paragraph is swallowed. **#618's (b) numbers are not affected by #621**: they measure H19 alone, and its lines are 784px wide at x=8, under the infobox. One limit: shape (b) will not show #621. The new `table_ws_rows_infobox.html` covers that, and the live Wikipedia infobox, whose rows are separated by newlines, needs both #619 and a #621 fix.

#621 says Reduce's `float_wrap_infobox_table.html` (#618) is affected. That page is the cloud session's, on #619, not Reduce's #618 page. At #619's current head it also writes its rows with no whitespace between them.

## Re-measure after the fix

After a #621 parser fix, re-run both pages here. Expected: RustKit shows 1 tbody, `<p>`s parented to `body`, and a table height near 140. After both #619 and #621, `p#p1` line 1 should be 8 / 433.48 beside the infobox, as in Chromium. The control page should change only with #619.
