## What

`get_elements_by_tag_name` / `get_elements_by_class_name` iterated `Document.nodes` (a `HashMap<NodeId, Rc<Node>>`), whose iteration order is randomized per process. Both now walk the tree from the root (pre-order DFS = document order).

## Why it matters (this is the flaky-parity bug)

Stylesheet extraction (`extract_stylesheets`) uses `get_elements_by_tag_name("style")` and silently depends on document order: CSS rule order breaks specificity ties. With random sheet order, the parity reset's `*, *::before, *::after { margin:0; padding:0 }` could land AFTER a fixture's element rules and zero their paddings/margins.

**Receipt:** identical `parity-capture` runs on the same fixture flipped between `body(20,20,560)` (correct) and `body(0,0,600)` (reset-wins) — 6 good / 4 broken over 10 runs on clean code. 10/10 stable after this fix. This retroactively explains run-to-run parity wobble we've both seen (same binary, same baseline, different diff %).

Windows note: if your capture path (or the minimal cascade you're building) reuses rustkit-dom's element lookup, you likely inherit the same nondeterminism — worth a 10-run flake check on one fixture after merging.

## Tests

- New regression test: 8 repeated parses assert stable head-then-body style order (and class-name order).
- rustkit-dom 52/52; rustkit-engine 16/16 + 19/19; rustkit-layout 223/223.

Session context: found while landing external-stylesheet loading in parity-capture (session-5 scope); with both changes the macOS unified pass rate moves 12/26 -> 15/26.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
