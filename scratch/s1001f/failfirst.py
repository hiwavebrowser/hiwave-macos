"""Fail-first: put the fix branch's rustkit-layout text.rs TEST MODULE onto develop's non-test code in the
develop worktree, run the two new tests there, then restore the file.
usage: failfirst.py <develop-worktree> <fix-worktree>"""
import subprocess, sys

dev, fix = sys.argv[1:3]
rel = "crates/rustkit-layout/src/text.rs"
orig = open(f"{dev}/{rel}").read()
new = open(f"{fix}/{rel}").read()
mark = "#[cfg(test)]\nmod tests {"
assert orig.count(mark) == 1 and new.count(mark) == 1
merged = orig[: orig.index(mark)] + new[new.index(mark):]
# develop has no INITIAL_FONT_FAMILY; the initial value there is the literal it replaced.
merged = merged.replace("rustkit_css::INITIAL_FONT_FAMILY", '"sans-serif"')
try:
    open(f"{dev}/{rel}", "w").write(merged)
    for t in ("generic_families_and_the_default_are_chromes_faces",
              "times_helvetica_and_courier_get_blinks_ascent_adjustment"):
        r = subprocess.run(["cargo", "test", "-p", "rustkit-layout", "--lib", t], cwd=dev,
                           capture_output=True, text=True)
        out = r.stdout + r.stderr
        keep = [l for l in out.splitlines()
                if "test result" in l or "panicked" in l or "left:" in l or "right:" in l or "Chrome" in l
                or l.startswith("error")]
        print(t)
        print("\n".join(keep[-8:]), flush=True)
finally:
    open(f"{dev}/{rel}", "w").write(orig)
print("restored:", open(f"{dev}/{rel}").read() == orig)
