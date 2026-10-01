"""Fail-first for the glyph-fallback fix: run the fix branch's layout integration test file, and the
rasteriser test that needs no new API, on develop's non-test code in the develop worktree; restore after.
usage: failfirst.py <develop-worktree> <fix-worktree>"""
import re, subprocess, sys

dev, fix = sys.argv[1:3]


def run(args):
    r = subprocess.run(args, cwd=dev, capture_output=True, text=True)
    out = r.stdout + r.stderr
    keep = [l for l in out.splitlines()
            if "test result" in l or "panicked" in l or "left:" in l or "right:" in l
            or l.startswith("error") or l.startswith("test ")]
    print("\n".join(keep[-14:]), flush=True)


# 1. layout: the whole integration test file (it uses only API develop has).
rel = "crates/rustkit-layout/tests/fallback_face_line_height.rs"
orig = open(f"{dev}/{rel}", newline="").read()
try:
    open(f"{dev}/{rel}", "w", newline="").write(open(f"{fix}/{rel}", newline="").read())
    print("== layout: a_symbol_courier_lacks_is_measured_in_its_cascade_face on develop")
    run(["cargo", "test", "-p", "rustkit-layout", "--test", "fallback_face_line_height"])
finally:
    open(f"{dev}/{rel}", "w", newline="").write(orig)
print("restored:", open(f"{dev}/{rel}", newline="").read() == orig)

# 2. rustkit-text: splice the one rasteriser test into develop's test module.
rel = "crates/rustkit-text/src/macos.rs"
orig = open(f"{dev}/{rel}", newline="").read()
new = open(f"{fix}/{rel}", newline="").read()
m = re.search(r"    /// Paint draws the fallback glyph from that same face.*?\n    }\r?\n\r?\n", new, re.S)
assert m, "rasteriser test not found"
anchor = "    #[test]\n    fn test_whitespace_transparent() {"
if anchor not in orig:
    anchor = anchor.replace("\n", "\r\n")
assert orig.count(anchor) == 1
try:
    open(f"{dev}/{rel}", "w", newline="").write(orig.replace(anchor, m.group(0) + anchor))
    print("== rustkit-text: the_rasteriser_draws_a_missing_symbol_from_the_cascade_face on develop")
    run(["cargo", "test", "-p", "rustkit-text", "--lib", "the_rasteriser_draws_a_missing_symbol"])
finally:
    open(f"{dev}/{rel}", "w", newline="").write(orig)
print("restored:", open(f"{dev}/{rel}", newline="").read() == orig)
