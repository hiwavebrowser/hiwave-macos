## The defect

`rustkit-engine`'s `match_pseudo_class` ended in `_ => true`: every pseudo-class the arm list did not name matched **every element**. That covered `:is()`/`:where()` (every modern CSS reset: `:where(ul, ol) { padding: 0 }` zeroed padding on every element), the whole `-of-type` family (`h2:first-of-type` styled every h2), `:link`/`:any-link`, `:placeholder-shown`/`:required`/`:invalid`/`:read-only`, `:has()`, `:lang()`, legacy `p:first-line` (styled the whole p), and other engines' vendor pseudo-classes (`:-moz-focusring`, which Chrome drops). `:empty` was hardcoded false. And the comma branch evaluated list members independently, where Selectors 4 §3.9 says one invalid member drops the whole rule.

Separately, `tokenize_selector` split at whitespace inside parentheses, so `:is(.a, .b)` became two tokens — `:is()` could never have worked even with an arm.

## The fix (engine only)

- **Validity gate** `selector_list_is_valid`: every `:name` must be one the matcher decides (`pseudo_class_is_supported`) or a legacy pseudo-element; `:is()`/`:where()` arguments are forgiving. An invalid list returns false before the comma split. The catch-all is now `_ => false` and unreachable.
- **`SiblingContext`** replaces the `(element_index, sibling_count)` pair through the build walk → style → matcher: adds the same-tag index/count and `has_children` (element or text child, whitespace included — §14.5). `create_pseudo_element` gets the host's real context instead of `(0, 1)`.
- **New arms**: `-of-type` family; `:is`/`:where`/`:matches`/`:-webkit-any` and `:not()` over a top-level-comma selector list; `:link`/`:any-link`; `:empty`; `:placeholder-shown`; `:required`/`:optional`/`:read-only`/`:read-write`/`:valid`/`:invalid`/`:in-range`; `:checked` limited to checkbox/radio/option; `:defined`; `:lang()`/`:dir()` from the own attribute; `:root`/`:scope`; `:has()` and document-state pseudo-classes false; `:first-line`/`:first-letter` false on the host.
- Tokenizer tracks parentheses; the comma split guards a member equal to the whole selector (unclosed paren used to recurse forever).

## Receipt — `parity-tests/repro/unknown-pseudo-class.html` vs pinned Chrome 148 (400×200)

| row | rule | Chrome | develop `da8f413` | this PR |
|---|---|---|---|---|
| A | `:is(.pick, .other)`, `:where(.pick)` | red blue | **red red** | red blue |
| B | `span:first-of-type`, `span:last-of-type`, `div:nth-of-type(2)` | red blue red blue green | **red red red green green** | red blue red blue green |
| C | `.sw:placeholder-shown`, `.sw:link`, `.sw:any-link`, `a {}` | blue blue red | **red red red** | blue blue red |
| D | `.sw:frobnicate`, `.sw:frobnicate, .keep` | blue blue | **red red** | blue blue |
| E | `.sw:empty`, `.sw:required`, `.sw:read-write` | red blue red | **red red red** | red blue red |

develop: 14 of 15 swatches wrong. This PR: 15/15 = Chrome.

## Boards

- Campaign 26/26 avg **2.6245 → 2.6279** on develop `da8f413` basis; 25 of 26 byte-flat. **settings +0.0895** is a revealed defect, not a regression: `.toggle input:checked + .toggle-slider::before { transform: translateX(22px); background: white }` never matched before (pseudo-elements were matched with an empty sibling list, so `+` failed) and now does — Chrome shows exactly that white knob at the right of each checked toggle. RustKit paints it 21px above the slider because the `::before` box's `bottom: 2px` resolves against the containing block's TOP (layout dump: knob y 289.19 for a slider at 310.19–336.19, before and after this PR). Ledgered for the layout lane; the census shows no board case uses any pseudo-class this PR adds, so the meter is blind to the fix itself.
- WPT Tier-1 24/26, same two fails, pinned on this branch's tip.

## Tests

rustkit-engine 82 → 87: unknown pseudo-class invalidates the whole rule (incl. `:-moz-focusring`, `:first-line`); `:is`/`:where`/`:not(list)`; `-of-type` typed index; `:link`/`:placeholder-shown`/`:empty` (whitespace-only span is NOT empty); validity + top-level comma splitting as pure functions.

## Ledger (not chased)

- `:is()`/`:where()`/`:not()` members with a combinator under-match (no ancestor chain in the compound matcher); `:has()` false; `:lang()`/`:dir()` read only the element's own attribute; structural pseudo-classes on ancestor/sibling compounds still permissive (n44 ledger); `:valid`/`:invalid` see only `required` + empty value.
- Absolutely positioned `::before`/`::after` with `bottom:` resolves against the containing block's top edge (settings toggle knob, above).

Forensics: `trench/forensics/2026-09-10-n45-unknown-pseudo-class-matched-everything.md` (hub repo).

🤖 Generated with [Claude Code](https://claude.com/claude-code)
