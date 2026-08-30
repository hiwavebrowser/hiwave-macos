# The unmerged engine branches — a manifest

**Measured 2026-08-30 against `develop` at `2be7d37`.** Every column below is a
command's output, not a recollection; the commands are at the bottom so this
file can be regenerated rather than trusted.

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

## The correction this manifest makes

The digests have priced the delay as **eight unmerged engine branches** since
2026-08-25. That count is wrong, and wrong in the direction that makes the
backlog look worse than it is: **three of the eight are already in `develop`**,
landed under an `-r2` successor branch while the original was left behind.

| left-behind branch | successor | in `develop`? |
|---|---|---|
| `atlas/abspos-overlay` | `atlas/abspos-overlay-r2` | **yes** |
| `atlas/glyph-raster-bearing` | `atlas/glyph-raster-bearing-r2` | **yes** |
| `atlas/webfont-load` | `atlas/webfont-load-r2` | **yes** |

Their heavy merge conflicts against `develop` (103 hunks on
`glyph-raster-bearing`) are the *evidence* of supersession, not a cost. **Do not
port from these three.** They are dead and should be deleted.

## The live pile

Five branches carry engine work that `develop` does not have. Two of them are
already open as PRs.

| branch | vs `develop` | crates commits | contains |
|---|---|---|---|
| **`atlas/percent-height-basis`** | CONFLICT (2 hunks) | 15 | chain tip — see below |
| **`atlas/replaced-flex-image-ratio`** | **clean** | 6 | chain tip — see below |
| **`atlas/abspos-shrink-to-fit`** | **clean** | 2 | out-of-flow `width: auto` is shrink-to-fit, not fill |
| **`atlas/inline-block-clip-baseline`** | **clean** | 1 | a clipping atomic inline's baseline is its bottom margin edge |
| `atlas/subpixel-flip` | clean | 1 | **NOT FOR MERGE** — its own commit says `MEASURED, FALSIFIED` |

Open PRs, both clean and both based on `develop`:

| PR | branch | contains |
|---|---|---|
| **#168** | `atlas/grid-grandchild-reflow` | a grid item's grandchildren size against the item, not the container |
| **#169** | `atlas/n36-text-overflow-ellipsis` | `text-overflow: ellipsis`; five css-text properties inherit into child elements |

## The two chains — they are stacked, not parallel

This is the structural fact the branch list hides. Four of the branch names
above are not independent work; each contains the one before it in full.

**Chain A** — `grid-item-subtree-width` ⊂ `p3-flex-residual` ⊂ `percent-height-basis`

Merging the tip merges all three. Its 15 crates commits include, in order:

```
d11ea6e test(engine): pin the height dispatch to parse_height_value
758d588 fix(layout): a nowrap run counts the collapsed space between its inline boxes
7b48db5 fix(engine): export the visual rect for transformed boxes
199a0ff test(layout): close the survivor the mutation sweep found
58366bc fix(layout): an absolute child's containing block is the ancestor's height, not the flow cursor
309e726 test(layout): close the three survivors the mutation sweep found
9fcfbdf fix(engine): every element box carries the oracle's join key, not just the generic path
c4c9328 fix(layout): an in-flow percentage height resolves against the parent, not the flow cursor
d711e89 test(layout): close the two survivors the mutation sweep found
```

Two entries are worth naming because other nights have gone looking for them:

- **`2e325e2`** (grid subtree re-flow) is the fix 2026-08-29 rewrote from
  scratch. It is now also on **#168**, cherry-picked with authorship kept — so
  merging #168 does *not* make chain A redundant, but it does mean the chain and
  the PR overlap on that commit.
- **`7b48db5`** — export the visual rect for transformed boxes — is **not in
  `develop`**, and it is the fix for the two axes #168 worsened
  (`sticky-scroll`'s `.overflow-content`, `x` 82.03 → 139.53). It was written
  the day its author hit that same 139.53.

**Chain B** — `replaced-border-box` ⊂ `replaced-aspect-ratio` ⊂ `replaced-flex-image-ratio`

Merging the tip merges all three. **The tip is clean against `develop`.** Its 6
crates commits:

```
e95f24b fix(layout): a replaced element carries its own box decoration
b2ad86e fix(engine): replaced elements and form controls carry a join key
00fcefb test(layout): guard the axis and the exported border box
8671ada test(engine): close the survivor — an untracked path stamps no identity
e0c8503 fix(layout): a specified aspect-ratio reaches replaced elements
118bca7 fix(layout): a flex item image derives its cross size from its ratio, not natural height
```

`b2ad86e` is an oracle-visible change: it gives replaced elements and form
controls a join key. Two cases in the corpus (`form-controls`, `images-intrinsic`)
carry 30 and 14 join failures respectively on the current floor, and elements
that fail to join are not compared at all — so this branch plausibly moves what
is *measurable*, not only what is correct. That should be measured before it is
claimed.

## What is measured about the pile's value, and what is not

Exactly one figure exists, and its scope is narrow:

> `atlas/grid-item-subtree-width`, merged onto `develop` in a throwaway branch
> on 2026-08-28: **2460 Gate A failures against develop's 2535** — 75 fewer
> failing axes. Linux/SwiftShader. **MECHANICS, NOT A RECEIPT.**

Nothing else in the pile has a number. Chain B in particular is clean, six
commits deep, and **entirely unmeasured** against the current tree. Its value is
a guess until a PR runs the gates on it.

## How to regenerate this file

```sh
git fetch origin '+refs/heads/atlas/*:refs/remotes/origin/atlas/*'

# ahead-of-develop, and how much of it touches the engine
for b in $(git for-each-ref --format='%(refname:short)' refs/remotes/origin/atlas/); do
  n=$(git rev-list --count "$b" ^origin/develop)
  [ "$n" = 0 ] && continue
  printf '%-45s ahead=%-4s crates=%s\n' "${b#origin/}" "$n" \
    "$(git rev-list --count "$b" ^origin/develop -- crates/)"
done

# does it merge clean?
git merge-tree --write-tree origin/develop origin/atlas/<branch> >/dev/null && echo clean || echo CONFLICT

# is it already in develop (the -r2 check)?
git merge-base --is-ancestor origin/atlas/<branch> origin/develop && echo MERGED

# is chain member X contained in chain tip Y?
git merge-base --is-ancestor origin/atlas/<X> origin/atlas/<Y> && echo "X in Y"
```

A branch that merges clean and has never been measured is the cheapest work
available to this campaign, and there are two of them.
