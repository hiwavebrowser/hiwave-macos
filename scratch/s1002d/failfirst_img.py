"""Fail-first for the image loader routing: append the branch's `image_loader_routing_tests` module to
develop's engine lib.rs (develop's code otherwise untouched), run it, restore the file.
usage: python3 failfirst_img.py <develop worktree> <fix worktree>"""
import os, subprocess, sys, time
dev, fix = sys.argv[1:3]
REL = 'crates/rustkit-engine/src/lib.rs'
src = open(f'{fix}/{REL}').read()
a = src.index('mod image_loader_routing_tests {')
a = src.rindex('// Needs a headless view', 0, a)
b = src.index('mod svg_content_type_tests {')
b = src.rindex('// Needs a headless view', 0, b)
module = src[a:b]
path = f'{dev}/{REL}'
orig = open(path).read()
assert 'image_loader_routing_tests' not in orig
try:
    open(path, 'w').write(orig + '\n' + module)
    r = subprocess.run(['python3', '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/s1002a/cerr.py', dev,
                        '-p', 'rustkit-engine', '--lib', '--features', 'headless', 'image_loader_routing'],
                       capture_output=True, text=True)
    print(r.stdout[-6000:], r.stderr[-2000:])
finally:
    open(path, 'w').write(orig)
    now = time.time()
    os.utime(path, (now, now))
print(subprocess.run(['git', 'status', '--short'], cwd=dev, capture_output=True, text=True).stdout or 'develop worktree clean')
