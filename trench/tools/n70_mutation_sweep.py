#!/usr/bin/env python3
"""Mutation sweep for n70's flex arm in `grid::own_min_content_width`.

A guard that stays green without its fix is decoration. Each probe replaces the
part of the arm it is about with a WRONG-BUT-PLAUSIBLE version, runs the night's
guards, and requires at least one of them to fail.

Two checks inherited from n69's sweep, both of which caught a broken sweep
rather than a broken guard on the nights they were added:

  * the mutation must land where it was AIMED. `own_min_content_width` and
    `own_max_content_width` are textually near-identical for several lines, and
    a first-occurrence replace lands in the sibling function twice in three
    nights of this trench.
  * each guard must be seen to RUN. `cargo test --lib <name> -- --exact` needs
    the full `tests::<name>` path; given a bare name it matches nothing, runs
    zero tests and exits 0, so every probe reports as a survivor.

  n70_mutation_sweep.py
"""
import pathlib, re, subprocess, sys, os

REPO = pathlib.Path(__file__).resolve().parents[2]
SRC = REPO / "crates/rustkit-layout/src/grid.rs"

ARM_HEAD = (
    "    if style.display.is_flex()\n"
    "        && style.flex_direction.is_row()\n"
    "        && matches!(style.flex_wrap, FlexWrap::NoWrap)\n"
    "    {"
)
# The gap line is IDENTICAL in `own_max_content_width`, and so are the two
# lines after it — the min arm is told apart only by `item_count` following
# `sum` where the max arm has `widest`. The first version of this anchor was
# two lines long and the sweep refused to start: "anchor GAP occurs 2 times".
# That is the misaim check doing its job on this sweep's first run.
GAP = ("        let main_gap = layout_box.length_to_px(&style.column_gap, 0.0);\n"
       "        let mut sum = 0.0f32;\n"
       "        let mut item_count = 0usize;")
SUM_RETURN = "        return sum + main_gap * item_count.saturating_sub(1) as f32 + padding_border;"
ITEM_READ = "            sum += estimate_min_content_width(child) + horizontal_margins(&child.style);"
OUT_OF_FLOW = (
    "            if matches!(\n"
    "                child.position,\n"
    "                crate::Position::Absolute | crate::Position::Fixed\n"
    "            ) {\n"
    "                continue;\n"
    "            }\n"
    "            // A white-space-only run never becomes a flex item either, so it"
)
WS_SKIP = (
    "            if let BoxType::Text(t) = &child.box_type {\n"
    "                if t.trim().is_empty() {\n"
    "                    continue;\n"
    "                }\n"
    "            }\n"
    "            sum += estimate_min_content_width(child)"
)
# The `width: Px` early return, anchored on the comment that PRECEDES it — that
# wording is unique to `own_min_content_width`, so the replace cannot land in the
# max-content twin, whose block is otherwise character-identical.
WIDTH_WINS = (
    "    // An explicit pixel width fixes the contribution regardless of content.\n"
    "    if let Length::Px(w) = style.width {\n"
    "        return match style.box_sizing {\n"
    "            BoxSizing::BorderBox => w,\n"
    "            BoxSizing::ContentBox => w + padding_border,\n"
    "        };\n"
    "    }"
)

GUARDS = [
    "a_row_flex_containers_min_content_sums_its_items_and_gaps",
    "min_content_sums_the_items_min_content_not_their_max_content",
    "a_wrapping_row_flex_container_min_content_takes_its_widest_item",
    "a_column_flex_container_min_content_takes_its_widest_item",
    "three_items_are_two_gaps_in_min_content",
    "the_row_gap_is_not_the_main_axis_gap_in_min_content",
    "a_percentage_gap_contributes_nothing_to_the_min_content_sum",
    "an_out_of_flow_child_is_not_a_flex_item_in_min_content",
    "a_whitespace_only_text_child_takes_no_gap_slot_in_min_content",
    "the_items_inline_margins_are_inside_the_min_content_sum",
    "the_containers_own_padding_and_border_are_added_to_the_min_content_sum",
    "a_specified_width_still_wins_over_the_min_content_flex_sum",
]

# (name, text to find, replacement). The find text must occur exactly once.
PROBES = [
    ("M1 no flex arm at all (the defect itself)", ARM_HEAD,
     "    if false {"),
    ("M2 the arm takes WRAPPING containers too", ARM_HEAD,
     "    if style.display.is_flex() && style.flex_direction.is_row() {"),
    ("M3 the arm takes COLUMN containers too", ARM_HEAD,
     "    if style.display.is_flex() && matches!(style.flex_wrap, FlexWrap::NoWrap) {"),
    ("M4 the gap is matched for `Length::Px` (n69's hole, reopened)", GAP,
     "        let main_gap = match style.column_gap {\n"
     "            Length::Px(g) => g,\n"
     "            _ => 0.0,\n"
     "        };\n        let mut sum = 0.0f32;\n        let mut item_count = 0usize;"),
    ("M5 a percentage gap resolves against the box's own used width", GAP,
     "        let main_gap =\n"
     "            layout_box.length_to_px(&style.column_gap, layout_box.dimensions.content.width);\n"
     "        let mut sum = 0.0f32;\n        let mut item_count = 0usize;"),
    ("M6 the main-axis gap is read from `row_gap`", GAP,
     "        let main_gap = layout_box.length_to_px(&style.row_gap, 0.0);\n        let mut sum = 0.0f32;\n        let mut item_count = 0usize;"),
    ("M7 one gap per ITEM instead of per boundary", SUM_RETURN,
     "        return sum + main_gap * item_count as f32 + padding_border;"),
    ("M8 exactly one gap, whatever the item count", SUM_RETURN,
     "        return sum + main_gap + padding_border;"),
    ("M9 the container's own padding and border are dropped", SUM_RETURN,
     "        return sum + main_gap * item_count.saturating_sub(1) as f32;"),
    ("M10 the item's MAX-content contribution is summed instead of its min", ITEM_READ,
     "            sum += estimate_max_content_width(child) + horizontal_margins(&child.style);"),
    ("M11 the item's inline margins are dropped", ITEM_READ,
     "            sum += estimate_min_content_width(child);"),
    ("M12 an out-of-flow child is counted as a flex item", OUT_OF_FLOW,
     "            // A white-space-only run never becomes a flex item either, so it"),
    ("M13 a white-space-only run is given a gap slot", WS_SKIP,
     "            sum += estimate_min_content_width(child)"),
    # M14's first form was a no-op block that changed no behaviour, and the
    # sweep reported it as a survivor whose guard was killed by nothing. The
    # claim the guard makes is about the arm's POSITION below the width check,
    # so the mutation has to be the one line that stops that position holding.
    ("M14 a specified width stops winning over the flex sum", WIDTH_WINS,
     "    if let Length::Px(w) = style.width {\n"
     "        if !style.display.is_flex() {\n"
     "            return match style.box_sizing {\n"
     "                BoxSizing::BorderBox => w,\n"
     "                BoxSizing::ContentBox => w + padding_border,\n"
     "            };\n"
     "        }\n"
     "    }"),
]

env = dict(os.environ)
env.setdefault("VK_ICD_FILENAMES",
               "/opt/pw-browsers/chromium-1194/chrome-linux/vk_swiftshader_icd.json")


def run_guards():
    """Run each guard alone. Returns (failing guards, how many actually RAN)."""
    fails, ran = [], 0
    for g in GUARDS:
        r = subprocess.run(["cargo", "test", "-p", "rustkit-layout", "--lib",
                            f"tests::{g}", "--", "--exact"],
                           cwd=REPO, capture_output=True, text=True, env=env)
        m = re.search(r"test result: \w+\. (\d+) passed; (\d+) failed", r.stdout)
        if m and int(m.group(1)) + int(m.group(2)) == 1:
            ran += 1
        if r.returncode != 0:
            fails.append(g)
    return fails, ran


original = SRC.read_text()
for label, text in (("ARM_HEAD", ARM_HEAD), ("GAP", GAP), ("SUM_RETURN", SUM_RETURN),
                    ("ITEM_READ", ITEM_READ), ("OUT_OF_FLOW", OUT_OF_FLOW),
                    ("WS_SKIP", WS_SKIP), ("WIDTH_WINS", WIDTH_WINS)):
    n = original.count(text)
    assert n == 1, f"anchor {label} occurs {n} times, not once"

print("control (unmutated):", flush=True)
ctrl, ran = run_guards()
print(f"  {'GREEN' if not ctrl else 'RED ' + str(ctrl)}  ({ran}/{len(GUARDS)} guards ran)", flush=True)
if ran != len(GUARDS):
    sys.exit("a guard did not run; every probe would report as a survivor")
if ctrl:
    sys.exit("control is red; the sweep cannot distinguish a probe from a broken tree")

rc = 0
killed_by = {g: [] for g in GUARDS}
try:
    for name, find, repl in PROBES:
        n = original.count(find)
        aim = "OK" if n == 1 else f"!!MISAIMED!! ({n} occurrences)"
        SRC.write_text(original.replace(find, repl, 1))
        landed = repl in SRC.read_text()
        fails, ran = run_guards()
        verdict = "RED (caught)" if fails else "GREEN — SURVIVOR"
        if not fails or n != 1 or not landed or ran != len(GUARDS):
            rc = 1
        print(f"\n{name}\n  aim={aim} landed={landed} ran={ran}/{len(GUARDS)}  {verdict}", flush=True)
        for f in fails:
            print(f"    caught by {f}")
            killed_by[f].append(name.split()[0])
        SRC.write_text(original)
finally:
    SRC.write_text(original)

print("\n# which probe kills which guard — a guard no probe kills is decoration")
for g, probes in killed_by.items():
    if not probes:
        rc = 1
    print(f"  {g:<70} {' '.join(probes) or '!! KILLED BY NOTHING !!'}")

print("\nrestored." if rc == 0 else "\nrestored. SWEEP HAS A SURVIVOR, A MISAIM OR AN UNKILLED GUARD.")
sys.exit(rc)
