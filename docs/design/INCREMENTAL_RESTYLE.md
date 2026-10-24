# Incremental restyle (design note, 2026-10-21)

Status: proposal, no code. Grounded in develop `13e25589`; `lib.rs` below means
`crates/rustkit-engine/src/lib.rs` unless another crate is named. Sites are referred to
by kind only.

## 0. The problem, measured

- A social-news site runs a ~0.48 s timer that makes 4–8 inline `style` writes per tick.
  Every tick is a relayout with `tree_reused=false`: median 474 ms, ~440 ms of it the
  box-tree build. Over 17 minutes the window thread was 97% busy.
- On a marketplace site, hover and click stall for 1.0–1.4 s each.
- Cascade sharing (on develop, `RUSTKIT_STYLE_SHARE`) took 13.8x off the cascade. That
  makes each full build cheaper but does not stop us doing a full build for every write.

**The premise needs one correction.** `building_view_has_edits()` (`lib.rs:7062`) is
not what turns tree reuse off after a script write. It covers only typed text in form
controls (`ViewState::edit_states`, `lib.rs:832`), which the memo key cannot see. The
real limit is that the style memo, and the tree reuse built on it, only exist while
`load_subresources` is running. `StyleMemoScope::arm()` (`lib.rs:5490`, `lib.rs:12657`)
drops the memo before any page script runs (`impl Drop for StyleMemoScope`). Its key,
`StyleMemoKey` (`lib.rs:32429`), has no DOM generation in it, and that is sound only
because no script runs inside the span. So after load **every** relayout walks the
whole document. `tree_reused=false` on a timer tick is the design working as intended,
and the fix is new machinery, not a relaxed gate.

## 1. Today's pipeline: from a DOM write to pixels

**Recording (bindings).** Script DOM writes go through two host functions:
`__rustkit_dom_write` (`crates/rustkit-bindings/src/dom.rs:1143`, which calls `write`
at `dom.rs:372`) and `__rustkit_dom_mutate` (`dom.rs:1125`, structure ops). Each write
returns a `DomDirty` bucket (`rustkit-bindings/src/lib.rs:560`), one of
`Clean < Layout < Style`, and the host folds it into **one page-wide `Cell`** with
`max`. The assignments, from `write`:

| write | bucket | site |
|---|---|---|
| `setAttr`/`removeAttr`, any name, including `style`, `class` and `id` (the style setter writes through it) | `Style` | `dom.rs:466-468` |
| text node `setText` | `Layout` | `dom.rs:470-478` |
| element `textContent`, `innerHTML`, `innerText` | `Style` | `dom.rs:483-529` |
| insert/remove/move | `Style` | `dom.rs:1125-1130` |
| control value | `Layout` (goes to `edit_states`) | `dom.rs:662` |

`ScriptMutations` (`lib.rs:618` in bindings) keeps only the **first** write's kind and
label plus a count per kind. It is diagnostics only. No node set, no attribute name and
no old value is recorded anywhere. Attribute writes call `Document::replace_node_data`
(`rustkit-dom/src/lib.rs:639`), which swaps in a new `Rc<Node>` but **keeps the
`NodeId`**. NodeIds are therefore stable keys for anything engine-side.

**Flush.** Once script settles, `flush_script_dom_writes` (`lib.rs:15585`) takes the
bucket. Any value other than `Clean` leads to `relayout` (`lib.rs:5853`). The comment
there says it plainly: "`relayout` rebuilds style and layout in full, so both
`DomDirty` buckets take the same path for now." Observer callbacks can add up to three
more flushes (`lib.rs:15592-15610`).

**Relayout** (`relayout_uncounted`, `lib.rs:5911`) runs these steps in order, every time:

1. `cache_inline_svgs`, then `build_layout_for_view` (`lib.rs:6723`) calls
   `build_layout_from_document` (`lib.rs:6769`).
2. Sheets. `extract_stylesheets` (`lib.rs:11687`) **re-parses the text of every
   `<style>` on every build**. Then come `keep_rules_for_viewport`,
   `assign_layer_order` and `extract_css_variables`. `shared_rule_index`
   (`lib.rs:13183`) reuses the last index, but only after comparing every selector
   string of every rule.
3. Walk. Starting at `<body>`, `build_layout_from_parent_style_and_path`
   (`lib.rs:7420`) calls `compute_style_for_element` (`lib.rs:8819`) for each element,
   which goes through `memoized_style` (a no-op when no memo is armed) and the
   style-sharing cache. The walk then builds a fresh `LayoutBox` tree
   (`rustkit-layout/src/lib.rs:1551`). `attach_identity` (`lib.rs:7271`) numbers
   elements **sequentially in walk order** (`element_ids`).
4. Layout. `layout_with_collapse` runs over the whole tree (`lib.rs:6004`). It has no
   dirty bits and no relayout boundaries.
5. `display_list_for` (`lib.rs:6127`) calls `DisplayList::build` over the whole tree,
   then splices in SVG.
6. `publish_geometry` (`lib.rs:18513`) publishes the whole tree to bindings. After
   that, `render` (`lib.rs:15505`).

**What is already incremental, and should be generalized.** Pointer restyle has a short
path, `repaint_restyled` (`lib.rs:6234`). It takes the elements whose `:hover`/`:active`
changed (`in_one_chain`, `lib.rs:3047`) and filters them through `SelectorReads`'
`PointerReads` (`lib.rs:31499`, `chain_change_restyles` at `lib.rs:3057`). Using
`RESTYLE_BUILD` (`RestyleBuild`, `lib.rs:32239`) it builds **only those subtrees**,
twice: once with the old chains and once with the new. It then compares them with
`restyle_needs_layout` (`lib.rs:32340`, built on `ComputedStyle::differs_only_in_paint`
at `rustkit-css/src/lib.rs:2546`), patches the laid-out tree with `copy_changed_paint`
(`lib.rs:32382`), rebuilds the display list and paints. That path needs no layout and
walks only the changed subtrees.

It **misses**, and falls back to a full rebuild, when:

- a rule reads the pseudo-class `anywhere` or through `siblings`;
- the focus moved, which every click does;
- the root element or body changed;
- any non-paint property changed. The paint list (`copy_paint_only`,
  `rustkit-css/src/lib.rs:2500`) deliberately excludes `opacity`, `transform` and
  `visibility`.

The miss reason is kept in `LAST_RESTYLE_MISS`. The marketplace stalls fit these
fallbacks. PR 1 confirms that by logging the reason.

## 2. Mutation taxonomy: what each write must invalidate

This uses the vocabulary of the dirty bits in §3. The key change from today is that
"restyle" becomes **"restyle the nodes in this set"**, with the set computed from an
invalidation index over the sheets' selectors.

| Mutation | Style dirty | Box/layout dirty | Notes |
|---|---|---|---|
| `style` attribute / `el.style.x =` | the element (`STYLE`). Its children only if an **inherited** property or a custom property changed value, decided after its restyle (§3.2) | Depends on the diff: paint-only ⇒ `PAINT`; otherwise `LAYOUT` on the element, which propagates (§3.4) | The timer case. Note that `[style]`/`[style*=…]` attribute selectors exist: treat `style` as an attribute too (next row). |
| `class` / `id` / other attribute | Look up each **changed token** (old Δ new class set; old and new id; the attribute name) in the invalidation index: subject ⇒ the element; ancestor compound ⇒ the element's subtree; sibling combinator or positional ⇒ later siblings and their subtrees; inside `:has()`/`:not()` with a combinator ⇒ the whole document | as above | Needs the old value, which bindings must record. A name no selector reads restyles nothing, which is the common case for `data-*` and `aria-*` toggles. |
| `:hover` / `:active` | today's `PointerReads`: the elements gained or lost, or the whole document if `anywhere`. Make `siblings` a sibling-subtree set rather than a miss | as above | Already works for paint-only changes. The gap is a layout-affecting hover, which today costs a full rebuild of the page instead of a restyle of the subtree. |
| `:focus` / `:focus-within` / `:focus-visible` | add a `focus` `PointerReads`. Old and new focused element plus (for `-within`) their ancestor chains | a caret ⇒ `PAINT` on the control | Removes the "focus moved" miss that every click hits. |
| text node data | none, unless `:empty` or a positional rule reads it (`SelectorReads::positional`) | `LAYOUT` on the parent box (its inline formatting context) | Bindings currently marks this `Layout`, but it still costs a full rebuild. |
| element `textContent` / `innerHTML` / `innerText` | the element's new subtree, which is all new nodes | `BOX` on the element (rebuild its children's boxes); `LAYOUT` upward | Plus a sibling rule, if the element's `:empty` state flipped. |
| insert / remove / move | inserted subtree: all `STYLE`. Siblings: per the positional and sibling sets (`:nth-*`, `:first/last-*`, `+`, `~`). Parent: `:empty`, and `:has()` ancestors | `BOX` on the parent; `LAYOUT` upward | A removal that takes the focused or hovered node is a pointer change too. |
| `<style>` text, `insertRule`, `<link>` added or removed, a sheet loaded | whole document (`STYLE` at the root) | full | Cascade sharing already makes this the cheap full case. Cache parsed sheets by text hash so step 2 does not re-parse unchanged `<style>` text (`web_cssom.js` writes `insertRule` into the `<style>` text). `adoptedStyleSheets` falls in this class when supported. |
| viewport resize, web font loaded, `@media` flip | whole document | full | Unchanged. These already have their own `RelayoutCause`s. |
| form value / checkedness / typed text | none, unless `:checked`/`:valid`/`:placeholder-shown` rules read it (the `MATCH_STATE_ATTRIBUTES` family) | the control's `BOX` (value text) | This is what `building_view_has_edits` stands for today. It becomes an ordinary dirty node. |

**The invalidation index** (new, built in `build_rule_index`, `lib.rs:13093`, next to
the existing `SelectorReads`). For each class, id and attribute name, record where it
appears as a bitset of `{subject, ancestor, sibling, anywhere}`, with one extra bit
saying whether the rule also reads position. `SelectorReads` already collects `ids`,
`attribute_tests` and `ancestor_attributes`, and it collects `PointerReads` for
`:hover`/`:active`. This adds classes and the sibling and anywhere axes, built by the
same compound parser. If a selector cannot be classified, its tokens get `anywhere`
(full restyle). Being conservative here costs time, never correctness.

## 3. Proposed design

### 3.1 Recording: a per-node mutation log

The page-wide `Cell<DomDirty>` and `ScriptMutations` are replaced by a `MutationLog`
in bindings. It is a `Vec<MutationRecord { node: usize, kind, attr: Option<name>,
old: Option<String> }>`. `old` is kept only for `class` and `id`. The log is capped at
about 4k records, and overflowing it sets `full = true`. `DomDirty` stays as the
summary for `__rustkit_dom_dirty` and the existing call sites. The engine takes the log
in `flush_script_dom_writes_once` and runs it through the invalidation index to get a
`DirtySet`. Pointer, focus, edit-state and sheet changes feed the same `DirtySet`, so
every cause goes down one path.

### 3.2 Dirty bits

The bits live engine-side, in a per-view `DirtySet`: a `HashMap<usize, Dirty>` keyed
by raw NodeId, because DOM node data is immutable and gets swapped by
`replace_node_data`.

- `STYLE`: recompute this element's cascade.
- `SUBTREE_STYLE`: recompute every descendant. Set by ancestor-compound invalidation
  and by an inherited-property change found during restyle.
- `DESCENDANT_DIRTY`: set on every ancestor of a dirty node. This is the walk `path`,
  exactly as `RestyleBuild.path` is today.
- `BOX`: the element's box subtree must be rebuilt. Set when its children changed, or
  when its `display`, `position`, `float`, pseudo-element content or anything else
  that decides box generation changed.
- `LAYOUT`: geometry may change. Propagates per §3.4.
- `PAINT`: only `copy_paint_only` properties changed.

`STYLE` decides `BOX`/`LAYOUT`/`PAINT` **after** the restyle, by comparing old and new
`ComputedStyle`. The comparison uses the existing `restyle_needs_layout` and
`differs_only_in_paint`. An inherited change sets `SUBTREE_STYLE` on the children, and
the change keeps cascading only as long as the children's styles keep changing. (When
the inherited set and the custom properties are equal, the children are clean.) This
is what makes a `color` write on `<body>` cost O(n) while a `top` write on a leaf costs
O(1).

### 3.3 Retained styles and boxes; restyle only the dirty subtrees

Two pieces of state are kept per view **across script**:

1. **Retained styles.** The per-node `ComputedStyle` (and pseudo styles) from the last
   build. This is the `StyleMemo` (`lib.rs:32463`) promoted from a load-span-scoped,
   thread-local memo to a per-view store with explicit invalidation. Replay uses the
   existing `memoized_style` hook: a node outside `STYLE ∪ SUBTREE_STYLE` replays.
   `StyleMemoKey` grows a `dom_generation`, and every mutation bumps it. A dirty node
   cascades and its entry is replaced. Any key field changing invalidates everything:
   viewport, fonts, sheets, and any new non-DOM input that `LaidWith`'s rule names
   (`lib.rs:32223`).
2. **Retained unlaid-out box tree.** This is `StyleMemo::tree`, likewise promoted.
   Rebuild only the `BOX`/`STYLE` roots, using `RESTYLE_BUILD` (walk `path`, build what
   is under `roots`, collect them in `found`), which already exists for hover. Then
   splice each fresh subtree into a clone of the retained tree at `box_of_node`. Clean
   subtrees keep their boxes. The new tree replaces the retained one.

The spliced tree is then laid out in full (§3.4 removes that later). From the
measurements, build is ~440 of 474 ms, so steps 1+2 alone take the timer case from a
full walk to a walk over a handful of elements plus a full layout.

**Root and body.** A dirty `<html>` or `<body>`, or a canvas-background change
(`lib.rs:6976-7019`), takes the full path. So does any shadow-host or slot change.
`shadow_tree::BuildScope` assigns slottables per build, so treat a host's subtree as
one unit at first.

**Element identity.** `element_ids` numbers boxes in walk order (`lib.rs:7282`), and
`StyleMemo::tree_images` is keyed by those numbers, as is parity capture. A spliced
subtree would number from 1. Option (a): key identity on NodeId raw and keep
`element_id` as a dense renumbering done in one cheap pass after the splice. Option
(b): renumber on splice. Option (a) is the recommendation. The selector path in
`attach_identity` depends on sibling position too, so an insert renumbers later
siblings' paths. Those paths are diagnostics only and can be recomputed lazily.

### 3.4 Layout dirtiness and containing blocks (PR 5, gated)

`LAYOUT` on a box marks that box, then walks up its containing-block chain, setting
`CHILD_LAYOUT` on each ancestor until it reaches a **relayout boundary**. A boundary is
a box whose size cannot depend on its content:

- explicit `width` and `height`, with `overflow` not `visible`;
- `position: absolute` or `fixed` with both dimensions fixed;
- the root.

Out-of-flow boxes propagate to their containing block, not their parent, using
`LayoutBox::containing_block_index`. Floats propagate to the block formatting context
root, because a float moves siblings' line boxes. Margin collapsing means that a
`LAYOUT` change on a child with collapsible margins also dirties the parent's
siblings. The rule for the first version is simple: any change to `margin-*` or to
in-flow block height propagates to the parent's parent.

Layout then skips clean subtrees (`!LAYOUT && !CHILD_LAYOUT`) whose containing-block
width is unchanged, and moves them by a delta if their position shifted. This is the
most invasive step, because `rustkit-layout` has no notion of reuse today
(`layout_with_collapse` threads `MarginCollapseContext` and `FloatContext` through
everything). For that reason it comes last, behind the oracle, and is optional: §0's
numbers do not need it.

### 3.5 The paint-only fast path, for every cause

The decision after `repaint_restyled`'s builds becomes a function of the `DirtySet`,
not of the pointer chains: "every dirty root rebuilt alike (`built_alike`) and
`differs_only_in_paint`". When that holds, it runs `copy_changed_paint` onto the
laid-out tree, then `display_list_for` and `render`, with **no layout**. A timer
writing `style.backgroundColor` or `style.color` lands here.

The paint list grows, one property at a time, each with its own oracle cases:

- `opacity`, when the stacking context does not change, meaning the old and new values
  are both `< 1` or both `= 1`;
- `visibility`, with a hit-test check: a hidden box stops being a hit target, but the
  boxes stay the same;
- `transform`, when a stacking context already exists. The view also recomputes
  `scrollable_bottom`/`max_scroll_offset`, because a transform moves scrollable
  overflow.

There is no per-box compositor layer today: `render` takes one display list per view.
So "composited transform" here means "geometry kept, display list rebuilt", not "GPU
layer moved". A real layer path is outside this note.

`publish_geometry` (`lib.rs:18513`) and `DisplayList::build` stay full-tree for now.
PR 1 measures them on a 5k-node page, and if either is over ~4 ms it becomes a partial
update in PR 4.

## 4. Invariants and test plan

### Invariants

- **I1 (the oracle).** For any DOM and any mutation sequence, the incremental
  pipeline's box tree, laid out, and its display list equal a full rebuild of the same
  DOM. "Equal" means `count_tree_differences` (`lib.rs:32395`) finds 0 differences and
  the display command lists are identical.
- **I2.** The `DirtySet` is a superset of the truly changed nodes: every node whose
  computed style differs between full builds before and after the mutation is in
  `STYLE ∪ SUBTREE_STYLE`.
- **I3.** Anything the build reads that is not the DOM or the sheets is in the retained
  state's key, or invalidates it. This is the rule `LaidWith` already states.
- **I4.** A fallback to the full path is always correct. Every unclassified case falls
  back, and logs why (extending `LAST_RESTYLE_MISS`).

### Tests

1. **Invalidation-superset test (I2)**, in PR 1, before any incremental code exists.
   Take a fixture page, run a full build, apply a mutation, run a second full build,
   diff the styles per node, and assert that the diff ⊆ the predicted `DirtySet`. This
   tests the invalidation index on its own.
2. **Randomized equivalence (I1).** A seeded generator, deterministic and with no new
   dependency (a small xorshift; `rustkit-engine` has no `proptest`/`rand` today),
   draws sequences of 1–50 mutations from the §2 taxonomy:
   - set or remove `class`/`id`/`data-*`/`style` with values drawn from the page's own
     selectors;
   - text edits;
   - insert, remove and move;
   - `innerHTML`;
   - hover, active and focus moves;
   - `<style>` appends.

   The generator runs over the `websuite/` micro pages plus three synthetic pages: a
   deep tree, a wide list with `:nth-child` rules, and a page with floats and margins.
   After each step it asserts I1. On failure it prints the seed and the minimal prefix.
   CI runs about 500 sequences; a `--ignored` soak runs 50k.
3. **Verify mode.** `RUSTKIT_INCREMENTAL=verify` follows the `RUSTKIT_TREE_REUSE=verify`
   precedent: it runs incremental and full builds side by side on real pages and
   `warn!`s with the mismatch count. We run it on the Mac against both sites before
   each PR leaves draft.
4. **Unit tests** per taxonomy row (each row of §2 gets at least one).
5. **Parity.** `python3 scripts/parity_test.py --scope micro` must be unchanged
   (incremental only differs after script runs, but `element_id` changes in 3.3 touch
   capture).

### Timing targets

These are measured by `relayout ... took_ms` and the stage split
(`relayout_cause::take_stage_ms`).

| Case | Today | Target |
|---|---|---|
| one inline-style write, layout-affecting, 5k-node page | full rebuild | **< 16 ms** relayout (one frame) |
| one inline-style write, paint-only, 5k-node page | full rebuild | < 4 ms |
| hover changing a layout property on a subtree of ≤ 200 nodes | full rebuild (~1 s on the marketplace) | **< 8 ms** |
| social-news timer tick (4–8 writes) | 474 ms median | < 16 ms p50, < 50 ms p95 |
| window-thread busy, 17 min on the news site | 97% | < 15% |

## 5. PR plan

Every PR is a draft, branched from develop, and stays under ~1,500 lines. Every PR is
measured on the Mac in the 02:00–03:00 quiet hour, with no other lanes running, using
the same script:

- a release build;
- the synthetic 5k-node timer page (added in PR 1 under `websuite/`) at 60 ticks;
- a 10-minute recorded hover sweep;
- 10 minutes on each live site;
- the outputs: p50/p95 `took_ms` by cause, `tree_reused`, the miss reasons, and
  window-thread busy %.

Every flag defaults **off** until its PR's verify run on both sites shows 0 mismatches.

1. **Record and classify, no behavior change (~1,200 lines).**
   - Bindings: `MutationLog` (§3.1), with old `class`/`id`.
   - Engine: invalidation index (§2), built in `build_rule_index`.
   - Engine: `DirtySet` computed and logged on each relayout (`dirty_nodes=` and
     `would_path=`).
   - Engine: the miss reason added to the relayout line.
   - Tests: the invalidation-superset test (Test 1), plus the synthetic 5k-node page
     and its timer fixture.

   Measured: dirty-set sizes and miss reasons on both sites. This confirms the hover
   stall cause and baselines every row of the targets table.
2. **Retained styles across script (~1,000 lines).**
   - Promote `StyleMemo` to per-view (§3.3 item 1), with `dom_generation`.
   - Cache parsed `<style>` sheets by text hash, and make the `shared_rule_index` check
     O(1) by keying it on that hash.
   - Add the randomized-equivalence harness (Test 2) and verify mode, behind
     `RUSTKIT_INCREMENTAL=styles`.

   Measured: `cascade_ms` and `parse_ms` on timer ticks. The expectation is that most
   of the 440 ms goes, leaving box construction and layout.
3. **Subtree box reuse (~1,400 lines).**
   - Retained unlaid-out tree, splice via `RESTYLE_BUILD` (§3.3 item 2).
   - NodeId-keyed identity with renumbering.
   - Root/body and shadow fallbacks.
   - Flag `=boxes`.

   Measured: build ms on ticks against the 16 ms target, which is reached here if
   layout is ≲ 10 ms. Otherwise PR 5 is needed.
4. **Paint-only and focus for every cause (~900 lines).**
   - Generalize `repaint_restyled` to take a `DirtySet`.
   - Focus reads (which removes the "focus moved" miss).
   - Sibling-set hover instead of a miss.
   - `opacity`/`visibility`/`transform` added to the paint list, one per commit, each
     with oracle cases.
   - Partial `publish_geometry` if PR 1 showed it hot.

   Measured: hover and click stalls on the marketplace site against the < 8 ms target,
   and paint-only ticks against < 4 ms.
5. **(Gated) Incremental layout (~1,500 lines).** Only if PR 3's numbers miss the
   targets. Layout dirty propagation and relayout boundaries (§3.4) in
   `rustkit-layout`. It ships behind `=layout`, with the soak at 50k sequences before it
   leaves draft.

## 6. Risks

- **Stale inputs (largest).** The build reads things that are not the DOM:
  `edit_states`, `checked_states`, focus, hover, viewport, font count, `svg_cache`,
  image sizes (`refresh_image_sizes`), `html_clips`, shadow slot assignment, and
  counters, quotes and `attr()` in `::before`/`::after` content. A missed input is a
  wrong frame that stays on screen until the next full build. Mitigations: I3, verify
  mode on real sites, and the oracle generator drawing from every §2 row.
- **Sibling- and position-dependent styles.** `:nth-child`, `+`/`~`, `:has()`, and
  `<ol>` numbering, where list markers count earlier siblings. An under-invalidation
  here hides from tests that only mutate leaves. Mitigation: classify conservatively
  (`anywhere` ⇒ full), and include a `:nth-child`-heavy synthetic page in the
  generator.
- **Margin collapsing and floats in incremental layout (PR 5).** These are where
  incremental layout engines get their worst bugs. That is the reason PR 5 is gated
  and optional.
- **Memory.** Retained styles cost ~1.5 KB per element (boxed `ComputedStyle`, per the
  `StyleMemo` comment). That is ~8 MB at 5k elements and ~17 MB at an 11k-element
  encyclopedia page, plus one unlaid-out tree. Style sharing lets entries share an
  `Rc`, so retain the shared `Rc` rather than a copy. The retained state is dropped on
  navigation and on memory pressure.
- **Cloning cost.** `view.layout` is an `Rc<LayoutBox>`. `Rc::make_mut` on a tree that
  `publish_geometry` or the inspector still holds clones the whole tree, silently
  turning O(dirty) back into O(n). Each PR's measurement must show build+splice time
  scaling with the dirty set, not with page size.
- **Thread-local state.** The memo is thread-local today (`STYLE_MEMO`), and the threads
  plan (`THREADS_PLAN_2026-10.md`) moves work between threads. Retained state must live
  in `ViewState`, not in a thread-local, so it moves with the view.
- **Observer loops.** `flush_script_dom_writes` re-flushes up to 3 times. A cheaper
  relayout makes observer feedback loops faster but not more frequent. No change, but
  watch the busy %.
- **Scope creep.** Compositor layers for transform and opacity animation are a separate
  project. This note stops at "geometry kept, display list rebuilt".
