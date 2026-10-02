"""Fail-first for the intrinsic letter-spacing fix: in the fix worktree, put develop's grid.rs back
under the new test, run it, then restore the fix. usage: failfirst_ls.py <fix-worktree>"""
import subprocess, sys

wt = sys.argv[1]
rel = "crates/rustkit-layout/src/grid.rs"
fixed = open(f"{wt}/{rel}", newline="").read()
base = subprocess.run(["git", "show", f"origin/develop:{rel}"], cwd=wt, capture_output=True).stdout.decode()
assert base and base != fixed
try:
    open(f"{wt}/{rel}", "w", newline="").write(base)
    r = subprocess.run(["cargo", "test", "-p", "rustkit-layout", "--lib",
                        "a_shrink_to_fit_box_is_as_wide_as_its_spaced_text"], cwd=wt, capture_output=True, text=True)
    out = r.stdout + r.stderr
    print("\n".join(l for l in out.splitlines()
                    if "test result" in l or "panicked" in l or " vs " in l or l.startswith("test ")
                    or l.startswith("error")), flush=True)
finally:
    open(f"{wt}/{rel}", "w", newline="").write(fixed)
print("restored:", open(f"{wt}/{rel}", newline="").read() == fixed)
