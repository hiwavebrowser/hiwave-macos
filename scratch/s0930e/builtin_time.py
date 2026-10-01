"""Time parity-capture on the shelf and chrome_rustkit builtin pages for each binary given, and
compare the frames across binaries (a hang check after a campaign 'Capture failed: Timeout').
usage: python3 builtin_time.py <bin>..."""
import filecmp, glob, os, subprocess, sys, time
SRC = '/Users/petecopeland/Repos/.worktrees/rs-flex-indef-column'
OUT = os.path.dirname(os.path.abspath(__file__)) + '/builtin'
os.makedirs(OUT, exist_ok=True)
pages = {}
for name in ('shelf', 'chrome_rustkit', 'chrome'):
    hits = glob.glob(f'{SRC}/crates/hiwave-app/src/ui/{name}.html')
    if hits:
        pages[name] = hits[0]
print('pages', pages)
frames = {}
for b in sys.argv[1:]:
    for name, path in pages.items():
        out = f'{OUT}/{name}-{os.path.basename(b)}.ppm'
        t = time.time()
        try:
            r = subprocess.run([b, '--html-file', path, '--dump-frame', out], capture_output=True, text=True, timeout=240)
            rc = r.returncode
        except subprocess.TimeoutExpired:
            rc = 'TIMEOUT'
        print(f'{os.path.basename(b):22s} {name:15s} rc={rc} {time.time()-t:.1f}s load={os.getloadavg()[0]:.1f}')
        frames.setdefault(name, []).append(out)
for name, fs in frames.items():
    if len(fs) > 1 and all(os.path.exists(f) for f in fs):
        print(name, 'frames identical:', all(filecmp.cmp(fs[0], f, shallow=False) for f in fs[1:]))
