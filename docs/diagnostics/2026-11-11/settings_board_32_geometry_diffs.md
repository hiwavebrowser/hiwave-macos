# Settings page on the chrome board: the 32 geometry diffs (2026-11-11)

**Question (Atlas):** the settings rows did not move with #1061 (CB3c button widths). All three sizes still show 32 geometry diffs and unchanged pixel %, although #1061's settings-button test passes. Why?

**Short answer:** none of the 32 diffs is a button size, before #1061 or after it. Every button on the board's settings page is already the size Chrome makes it, so #1061 had nothing to change there. The diffs come from three other causes. The settings rows are already green on pixels (0.157–0.215 %).

## How this was measured
- Board: `scripts/chrome_board.py --only settings`, run with two `parity-capture` builds:
  - develop **feeaf0ca** (includes #1061), built release in this checkout;
  - the seed **2ccb3325** (before #1061).
- Oracle: the board's Chrome for Testing 148.0.7778.216 captures.
- Geometry: `layout_oracle_gate.compare_case` on each size's `layout-rects.json` against `rustkit-layout.json`.
- **Result:** the diff lists are identical, entry for entry, on both builds and at all three sizes (1280x800, 1024x700, 1600x1000; the container's max-width makes the page the same at every size). Pixel %: 0.176 / 0.215 / 0.157 on both builds.

## The 32 diffs, grouped by cause

### A. `<select>` content line 2 px short (16 diffs). Not buttons.
- Five selects are 33 tall where Chrome makes them 35 (`height`, plus `y` +1 to +1.3 from centring): `#theme`, `#defaultMode`, `#tabDecayUnit`, `#analyticsRetention`, `#analyticsReportDay`.
- Three `.setting-control` wrappers (the flex boxes that centre `#theme`, `#defaultMode` and `#tabDecayUnit`) carry the same height and y deltas.
- Count: 5 × (y, height) + 3 × (y, height) = 16.

Reduced (`select{font-size:13.6px;padding:8px 12px;border:1px solid}` with one option):

| | height |
|---|---|
| CfT 148 | 35 |
| RustKit feeaf0ca | 33 |
| Default select, no author padding | 19 in both |

So with author padding, Chrome's content line is 17 at 13.6 px Arial, where RustKit uses Arial's `normal` line (15) in `form_control_intrinsic_size` (`normal_line_height + author padding + border`). The select rule is separate from the button rule #1061 rewrote. **Next step:** pin the menulist's content line in Chrome at a few font sizes, with and without author padding, then fix in the Select arm.

### B. `navigator.platform` text (1 diff). Fixed by #1069 (batch 61).
- `#focusShortcutDesc` is 261.4 wide; Chrome makes it 279.2.
- The page writes `Press ⌘⇧↵ …` when `navigator.platform` contains MAC. RustKit said "Win32" and showed `Press F11 …`.
- #1069 makes it "MacIntel", which should clear this diff. It is the same cause as the chrome page's focus hint (CB6b).

### C. Line pitch of a small inline with its own `line-height` inside a larger strut (2 diffs here, 13 downstream).
`section:nth-of-type(10) > .setting-label:nth-of-type(6) > span` is the Privacy Notice paragraph: `font-size: 0.8rem; line-height: 1.5` inside a 16 px system-ui block. It is 53.4 tall where Chrome makes it 55.4 (`height`, and `y` −0.6).

Reduced, 3 lines at 400 px:

| Font | CfT 148 | RustKit |
|---|---|---|
| system-ui span in a system-ui block | 55.375 | 53.4 |
| Helvetica span in a Helvetica block | 53.375 | (not checked) |

The 2 px is the line pitch, 20.19 vs 19.2:
- In Chrome the line box is the union of the parent's strut (16 px system font, `normal`) and the span's 19.2 px inline box, aligned on the baseline. With the system font's metrics that union is about 1 px taller than 19.2 per line.
- RustKit spaces the lines at the span's 19.2.

Downstream of C, 13 diffs:
- Heights of the label, `section:nth-of-type(10)`, `body > div.container` and `html > body` (4). The section and container deltas also include A's 2 px per select.
- The `y` of everything below the paragraph (9): `.btn-group` (7th child of section 10), `#viewReportBtn`, `#exportAnalyticsBtn`, `#clearAnalyticsBtn`, `.footer`, `span.version`, the footer `.btn-group`, `#saveBtn`, `#closeBtn`.

**Which diffs are buttons:** the five buttons in that list (`#viewReportBtn`, `#exportAnalyticsBtn`, `#clearAnalyticsBtn`, `#saveBtn`, `#closeBtn`). They are only moved down: `y` −2.6, no `x`, `width` or `height` delta.
- All 14 buttons on the page match Chrome in size on both builds. The two `.remove-btn` are 17.3 × 16; the second differs by 0.1 px in x, under the 0.5 px tolerance.
- That is why #1061 moved nothing on this page. Its test page (`cb3c_settings_remove_button.html`) uses style combinations the board's settings fixture does not reach.

### Tally
| Cause | Diffs | Fixed by |
|---|---|---|
| A. select content line | 16 | open (new row) |
| B. `navigator.platform` text | 1 | #1069 |
| C. line pitch, strut + smaller inline line-height (2) and everything below it (13) | 15 | open (new row) |
| **Total** | **32** | |

## Not done
- Nothing is fixed here.
- The select's 17 px content line is measured at one size only.
- C's union rule is inferred from two fonts (system-ui shows the extra pitch, Helvetica does not). It needs a CfT matrix over font sizes and parent fonts before a fix.
