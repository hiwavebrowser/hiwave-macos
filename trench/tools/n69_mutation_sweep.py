#!/usr/bin/env python3
"""Mutation sweep for n69's gap resolution in `grid::own_max_content_width`.

A guard that stays green without its fix is decoration. Each probe replaces the
one line the change landed on with a WRONG-BUT-PLAUSIBLE version, runs the
night's guards, and requires at least one of them to fail. It also asserts the
mutation landed where it was aimed (09-27's finding: a first-occurrence replace
can hit the identically-worded line in a sibling function and report a guard as
sound on evidence that never touched it).

  n69_mutation_sweep.py
"""
import pathlib, re, subprocess, sys, os

REPO = pathlib.Path(__file__).resolve().parents[2]
SRC = REPO / "crates/rustkit-layout/src/grid.rs"
FIX = "        let main_gap = layout_box.length_to_px(&style.column_gap, 0.0);"

GUARDS = [
    "a_rem_gap_contributes_to_a_flex_containers_max_content",
    "an_em_gap_resolves_against_the_containers_own_font_size",
    "a_percentage_gap_contributes_nothing_to_an_intrinsic_contribution",
    "a_viewport_gap_resolves_against_the_viewport_width",
    "a_rem_gap_is_counted_once_per_item_boundary_not_once_per_item",
    "the_row_gap_is_not_the_main_axis_gap_of_a_row_flex_container",
    "a_column_direction_flex_container_takes_the_widest_item_and_no_gap",
    "a_rem_gap_does_not_reach_the_contribution_past_an_explicit_width",
]

# (name, text to find, replacement). The find text must occur exactly once.
PROBES = [
    ("M1 revert to the Px-only match (the defect itself)", FIX,
     "        let main_gap = match style.column_gap {\n"
     "            Length::Px(g) => g,\n"
     "            _ => 0.0,\n"
     "        };"),
    ("M2 percentages resolve against the box's own used width", FIX,
     "        let main_gap = layout_box.length_to_px(&style.column_gap, layout_box.dimensions.content.width);"),
    ("M3 every relative gap goes through the ROOT font size (em read as rem)", FIX,
     "        let main_gap = style.column_gap.to_px_with_viewport(16.0, 16.0, 0.0, layout_box.viewport.0, layout_box.viewport.1);"),
    ("M4 viewport units dropped (`to_px` instead of `length_to_px`)", FIX,
     "        let main_gap = style.column_gap.to_px(16.0, 16.0, 0.0);"),
    ("M5 the main-axis gap is read from `row_gap`", FIX,
     "        let main_gap = layout_box.length_to_px(&style.row_gap, 0.0);"),
    ("M6 one gap per ITEM instead of per boundary",
     "            sum + main_gap * item_count.saturating_sub(1) as f32",
     "            sum + main_gap * item_count as f32"),
    ("M7 the gap is added to a COLUMN container's width too",
     "        } else {\n            widest\n        };",
     "        } else {\n            widest + main_gap * item_count.saturating_sub(1) as f32\n        };"),
    # M8 exists because the first sweep left `a_rem_gap_does_not_reach_the
    # _contribution_past_an_explicit_width` dying under no probe at all — a
    # guard no mutation can kill is decoration, whatever it asserts. The flex
    # arm's position BELOW the `width: Px` check is the thing it guards, and
    # this is the one-line way that position stops holding.
    # Anchored on the comment that FOLLOWS the check, because the block itself
    # is textually identical in `own_min_content_width` and a first-occurrence
    # replace lands there instead — 09-27's misaim, reproduced exactly on the
    # first run of this probe (aim=!!MISAIMED!! (2 occurrences), and the tree
    # then failed to compile so every guard reported as "caught").
    ("M8 a specified width stops winning over a flex container's items",
     "    if let Length::Px(w) = style.width {\n"
     "        return match style.box_sizing {\n"
     "            BoxSizing::BorderBox => w,\n"
     "            BoxSizing::ContentBox => w + padding_border,\n"
     "        };\n"
     "    }\n"
     "\n"
     "    // A form control's content is not in its children: a <button>'s text lives",
     "    if let Length::Px(w) = style.width {\n"
     "        if !style.display.is_flex() {\n"
     "            return match style.box_sizing {\n"
     "                BoxSizing::BorderBox => w,\n"
     "                BoxSizing::ContentBox => w + padding_border,\n"
     "            };\n"
     "        }\n"
     "    }\n"
     "\n"
     "    // A form control's content is not in its children: a <button>'s text lives"),
    # M9 and M10 are the narrow forms of M6 and M5. Under the broad versions
    # five guards fail together, so neither the three-item guard nor the
    # row-gap guard was load-bearing for anything the others did not already
    # cover. These kill exactly one guard each.
    ("M9 exactly one gap, whatever the item count",
     "            sum + main_gap * item_count.saturating_sub(1) as f32",
     "            sum + main_gap"),
    ("M10 the main-axis gap is the LARGER of the two gaps",
     "        let main_gap = layout_box.length_to_px(&style.column_gap, 0.0);",
     "        let main_gap = layout_box\n            .length_to_px(&style.column_gap, 0.0)\n            .max(layout_box.length_to_px(&style.row_gap, 0.0));"),
]

env = dict(os.environ)
env.setdefault("VK_ICD_FILENAMES",
               "/opt/pw-browsers/chromium-1194/chrome-linux/vk_swiftshader_icd.json")

def run_guards():
    """Run each guard alone. Returns (failing guards, how many actually RAN).

    The run count is not bookkeeping. `cargo test --lib <name> -- --exact` needs
    the full `tests::<name>` path; given the bare name it matches nothing, runs
    zero tests and exits 0 — so every probe, including deleting the fix outright,
    reports as a survivor. The first version of this sweep did exactly that. A
    sweep that cannot tell "the guard passed" from "the guard never ran" is
    worth less than no sweep, so the count is asserted rather than assumed.
    """
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
assert original.count(FIX) == 1, "the fix line is not where the sweep expects it"

print("control (unmutated):", flush=True)
ctrl, ran = run_guards()
print(f"  {'GREEN' if not ctrl else 'RED ' + str(ctrl)}  ({ran}/{len(GUARDS)} guards ran)", flush=True)
if ran != len(GUARDS):
    sys.exit("a guard did not run; every probe would report as a survivor")
if ctrl:
    sys.exit("control is red; the sweep cannot distinguish a probe from a broken tree")

rc = 0
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
        SRC.write_text(original)
finally:
    SRC.write_text(original)

print("\nrestored." if rc == 0 else "\nrestored. SWEEP HAS A SURVIVOR OR A MISAIM.")
sys.exit(rc)
