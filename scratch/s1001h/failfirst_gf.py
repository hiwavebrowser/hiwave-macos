"""Fail-first for the grid flex-item fix: switch off its two production changes in grid.rs (the stretch
restore in Phase 9 and the Phase 9.8 re-layout), run the four new tests, then restore the file.
usage: failfirst_gf.py <fix-worktree>"""
import subprocess, sys

wt = sys.argv[1]
rel = "crates/rustkit-layout/src/grid.rs"
fixed = open(f"{wt}/{rel}", newline="").read()
a = "if area_height > content_height\n"
b = "if used - content_height > 0.01 {"
assert fixed.count(a) == 1 and fixed.count(b) == 1, (fixed.count(a), fixed.count(b))
off = fixed.replace(a, "if false && area_height > content_height\n").replace(b, "if false && used - content_height > 0.01 {")
tests = ["a_flex_grid_item_fills_its_row_and_aligns_in_it", "a_column_flex_grid_item_justifies_in_its_row",
         "a_start_aligned_flex_grid_item_keeps_its_content_height", "a_flex_grid_item_realigns_after_its_row_grows"]
try:
    open(f"{wt}/{rel}", "w", newline="").write(off)
    r = subprocess.run(["cargo", "test", "-p", "rustkit-layout", "--lib", "flex_grid_item"], cwd=wt,
                       capture_output=True, text=True)
    out = r.stdout + r.stderr
    print("\n".join(l for l in out.splitlines()
                    if "test result" in l or "panicked" in l or "got " in l or l.startswith("test ")
                    or l.startswith("error")), flush=True)
finally:
    open(f"{wt}/{rel}", "w", newline="").write(fixed)
print("restored:", open(f"{wt}/{rel}", newline="").read() == fixed)
