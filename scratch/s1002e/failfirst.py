"""Fail-first for the two branches of this session: put each new test into DEVELOP's engine lib.rs
(code otherwise untouched), run it through cargo-serial, restore the file.
usage: failfirst.py fetch|lp"""
import pathlib, subprocess, sys
W = '/Users/petecopeland/Repos/.worktrees'
dev = pathlib.Path(f'{W}/rs-dev-9f49a40/crates/rustkit-engine/src/lib.rs')
orig = dev.read_bytes()
which = sys.argv[1]
try:
    s = orig.decode()
    if which == 'fetch':
        mod = pathlib.Path(f'{W}/trench-realsite/scratch/s1002e_test.rs').read_text()
        s = s + '\n' + mod
        flt = ['background_image_fetch']
    else:
        fix = pathlib.Path(f'{W}/rs-control-semantics/crates/rustkit-engine/src/lib.rs').read_text()
        a = fix.index('    // Three 60x60 blue boxes down the left edge, 20px apart')
        b = fix.index('\n}\n', a)
        anchor = 'assert_eq!(pixel(&ppm, 70, 250), WHITE, "inside the shadowed box");\n    }\n'
        i = s.index(anchor) + len(anchor)
        s = s[:i] + '\n' + fix[a:b] + s[i:]
        flt = ['a_url_background_sits']
    dev.write_bytes(s.encode())
    r = subprocess.run(['python3', f'{W}/trench-realsite/scratch/s1002e/c.py', 'rs-dev-9f49a40', '--touch', 'test',
                        '-p', 'rustkit-engine', '--lib', '--features', 'headless'] + flt,
                       capture_output=True, text=True)
    print(r.stdout[-6000:], r.stderr[-2000:])
finally:
    dev.write_bytes(orig)
    print('restored', dev)
