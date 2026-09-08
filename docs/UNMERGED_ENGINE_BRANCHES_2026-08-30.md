# The unmerged engine branches — a manifest

**Re-measured 2026-09-01 against `develop` at `5b89ed8`.**
First measured 2026-08-30 against `develop 2be7d37`; that base has since absorbed
#168 and #169, so the earlier reading is kept only where it is still the record
of something. Every column below is a command's output, not a recollection; the
commands are at the bottom so this file can be regenerated rather than trusted.

## Why this file exists

Two nights were spent on the absence of an index.

- **2026-08-28** checked `atlas/grid-item-subtree-width` for one box, found it
  did not fix that box, and recorded that the pile did not contain the fix.
- **2026-08-29** independently rewrote a fix that was sitting on that same
  branch, tested and mutation-checked, fifteen days old — then reproduced its
  measurement to the decimal (−35 axes, `sticky-scroll` 149 → 114, the same two
  worsened boxes, the same 139.53px) before discovering it.

Both readings were locally correct. The digest's conclusion was the honest one:
*a branch name is not an index, and eight branches deep the commit subjects are
the only index there is.* This is the index.

## The `-r2` supersession pattern — now four instances

The digests priced the delay as **eight unmerged engine branches** from 08-25.
That count was wrong in the direction that makes the backlog look worse than it
is: several of those branches are dead, replaced by an `-r2` successor while the
original was left behind. **Their heavy merge conflicts against `develop` are
the *evidence* of supersession, not a cost.**

| left-behind branch | successor | successor's state |
|---|---|---|
| `atlas/abspos-overlay` | `atlas/abspos-overlay-r2` | **in `develop`** |
| `atlas/glyph-raster-bearing` | `atlas/glyph-raster-bearing-r2` | **in `develop`** |
| `atlas/webfont-load` | `atlas/webfont-load-r2` | **in `develop`** |
| `atlas/n37-ws-line-box` (PR #171, **closed**) | `atlas/n37-ws-line-box-r2` (**PR #174**, open, clean) | not yet in `develop` |

**Do not port from the left-hand column.** The first three are fully superseded
and should be deleted. The fourth is live work that simply moved branches.

The pattern has now recurred four times, which makes it a property of how this
repo lands engine work rather than an accident. A branch that conflicts heavily
with `develop` should be checked for an `-r2` **before** anyone tries to resolve
those conflicts.

## Landed since the first measurement

Out of the pile entirely — merged into `develop`:

| PR | branch | what it carried |
|---|---|---|
| **#168** | `atlas/grid-grandchild-reflow` | a grid item's grandchildren size against the item, not the container |
| **#169** | `atlas/n36-text-overflow-ellipsis` | `text-overflow: ellipsis`; five css-text properties inherit into child elements |

## The live pile, against `develop 5b89ed8`

| branch | vs `develop` | crates commits | note |
|---|---|---|---|
| **`atlas/n29-replaced-chain-port`** (**PR #175**) | **clean** | 7 | Chain B, ported forward — see below |
| **`atlas/n37-ws-line-box-r2`** (**PR #174**) | **clean** | 3 | successor to the closed #171 |
| **`atlas/n38-inline-svg-paint`** (**PR #173**) | CONFLICT | 5 | **stacked on a closed branch — see below** |
| **`atlas/percent-height-basis`** | CONFLICT | 15 | Chain A tip — see below |
| **`atlas/abspos-shrink-to-fit`** | **clean** | 2 | out-of-flow `width: auto` is shrink-to-fit, not fill |
| **`atlas/inline-block-clip-baseline`** | **clean** | 1 | a clipping atomic inline's baseline is its bottom margin edge |
| `atlas/subpixel-flip` | clean | 1 | **NOT FOR MERGE** — its own commit says `MEASURED, FALSIFIED` |

Dead (superseded, per the table above): `abspos-overlay`, `glyph-raster-bearing`,
`webfont-load`, `n37-ws-line-box`. Chain A's interior branches
(`grid-item-subtree-width`, `p3-flex-residual`) are listed under Chain A rather
than separately.

### PR #173 is stacked on a branch that is closed

`atlas/n38-inline-svg-paint`'s base is `atlas/n37-ws-line-box` — the branch whose
own PR (#171) was **closed** in favour of `-r2`. So #173 targets a base that is
not going anywhere, and its diff is measured against a tree `develop` will never
contain. It also conflicts with `develop` directly. Whoever owns it should
re-target it onto `atlas/n37-ws-line-box-r2` (#174) or onto `develop`; this file
does not do that, because re-targeting someone else's PR is theirs to decide.

### Chain B has been ported forward — this is #175

**Chain B** was `replaced-border-box` ⊂ `replaced-aspect-ratio` ⊂
`replaced-flex-image-ratio`, and on 08-30 this file called it *"clean against
develop, six commits deep, never measured by any gate — the cheapest work
available to this campaign."* It has since been rebased onto current `develop`
as **`atlas/n29-replaced-chain-port` (PR #175)**, clean, with the same six
commits plus a guard:

```
5fc39cb fix(layout): a replaced element carries its own box decoration
116f1f5 fix(engine): replaced elements and form controls carry a join key
1f963a0 test(layout): guard the axis and the exported border box
55b63aa test(engine): close the survivor — an untracked path stamps no identity
8de3c38 fix(layout): a specified aspect-ratio reaches replaced elements
64ec5a1 fix(layout): a flex item image derives its cross size from its ratio
9eb42c0 test(engine): the GPU-gated identity guard says when it skipped
```

`116f1f5` is the oracle-visible one: it gives replaced elements and form controls
a join key. `form-controls` and `images-intrinsic` carried 30 and 14 join
failures on master's floor, and **an element that fails to join is never
compared at all** — so this branch plausibly moves what is *measurable*, not only
what is correct. The old three branches remain on the remote and are now
redundant with #175.

### Chain A — still unmerged, still conflicting, and now partly redundant

**Chain A** is `grid-item-subtree-width` ⊂ `p3-flex-residual` ⊂
`percent-height-basis`. Merging the tip merges all three; the tip conflicts with
`develop`.

Two things changed under it since 08-30, and they pull in opposite directions:

- **Part of it has landed by another route.** #168 cherry-picked `2e325e2` (the
  grid subtree re-flow) as `ea6d4ca`, which **is** now in `develop`. The original
  `2e325e2` is **not** an ancestor of `develop`, so a SHA-based containment check
  still reports the whole chain as unmerged — it is not. Anyone resolving this
  chain's conflicts should expect that commit to be redundant.
- **The commit that is still worth the most is still not in `develop`.**
  `7b48db5` — export the visual rect for transformed boxes — remains absent, and
  it is the fix for the two axes #168 worsened (`sticky-scroll`'s
  `.overflow-content`, `x` 82.03 → 139.53).

## What is measured about the pile's value, and what is not

Exactly one figure exists, and its scope is narrow:

> `atlas/grid-item-subtree-width`, merged onto `develop` in a throwaway branch
> on 2026-08-28: **2460 Gate A failures against develop's 2535** — 75 fewer
> failing axes. Linux/SwiftShader. **MECHANICS, NOT A RECEIPT.**

That figure now partly double-counts, since #168 landed the largest single fix
in it. Nothing else in the pile has a number. **#175 in particular is clean,
seven commits deep, and unmeasured against any gate** — its PR run is what would
settle it.

## How to regenerate this file

```sh
git fetch origin '+refs/heads/atlas/*:refs/remotes/origin/atlas/*' develop

# ahead-of-develop, engine-touching, and whether it merges clean
for b in $(git for-each-ref --format='%(refname:short)' refs/remotes/origin/atlas/); do
  nc=$(git rev-list --count "$b" ^origin/develop -- crates/); [ "$nc" = 0 ] && continue
  git merge-tree --write-tree origin/develop "$b" >/dev/null 2>&1 \
    && s=clean || s=CONFLICT
  printf '%-42s crates=%-3s %s\n' "${b#origin/}" "$nc" "$s"
done

# already in develop (the -r2 check — run this BEFORE resolving any conflict)
git merge-base --is-ancestor origin/atlas/<branch> origin/develop && echo MERGED

# is chain member X contained in chain tip Y?
git merge-base --is-ancestor origin/atlas/<X> origin/atlas/<Y> && echo "X in Y"
```

A SHA check answers "is this commit in develop", not "is this *change* in
develop" — a cherry-pick lands the change under a new SHA and the check still
says no. Chain A is the live example.

A branch that merges clean and has never been measured is the cheapest work
available to this campaign. On 08-30 there were two; the recommendation was
acted on, and **#175 is now that branch.**
