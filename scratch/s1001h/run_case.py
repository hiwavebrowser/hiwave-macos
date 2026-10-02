"""Run a parity-capture binary on campaign cases directly; print rc, seconds and the stderr tail.
usage: run_case.py <binary> <case> [...]"""
import glob, subprocess, sys, time
SRC = '/Users/petecopeland/Repos/.worktrees/rs-dev-9f49a40'
for case in sys.argv[2:]:
    hits = [p for p in glob.glob(f'{SRC}/websuite/**/{case}/index.html', recursive=True)
            + glob.glob(f'{SRC}/websuite/**/{case}.html', recursive=True)]
    if not hits:
        print(case, 'no fixture found')
        continue
    t = time.time()
    try:
        r = subprocess.run([sys.argv[1], '--html-file', hits[0], '--width', '1280', '--height', '800',
                            '--dump-frame', f'/tmp/rc-{case}.ppm'], capture_output=True, text=True, timeout=170)
        print(case, hits[0].replace(SRC, ''), 'rc', r.returncode, f'{time.time()-t:.0f}s', r.stderr[-600:].strip())
    except subprocess.TimeoutExpired:
        print(case, 'TIMEOUT 170s')
