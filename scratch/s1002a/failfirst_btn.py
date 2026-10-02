"""Fail-first for the button box fix: splice the fix branch's new tests into develop's test modules
(non-test code untouched), run them, restore the files.
usage: failfirst_btn.py <develop-worktree> <fix-worktree>"""
import os, subprocess, sys

dev, fix = sys.argv[1:3]
ENV = dict(os.environ, CARGO_TARGET_DIR='/Users/petecopeland/Repos/.worktrees/rs-dev-bf806c5/target')


def run(args):
    r = subprocess.run(args, cwd=dev, env=ENV, capture_output=True, text=True)
    out = r.stdout + r.stderr
    keep = [l for l in out.splitlines()
            if 'test result' in l or 'panicked' in l or l.startswith('error') or l.startswith('test ')
            or 'Chrome' in l or 'got ' in l]
    print('\n'.join(l[:260] for l in keep[-24:]), flush=True)


def between(s, start, end):
    i = s.index(start)
    return s[i:s.index(end, i)]


def splice(rel, start, end, anchor, label, args):
    f = open(f'{fix}/{rel}', newline='').read()
    orig = open(f'{dev}/{rel}', newline='').read()
    block = between(f, start, end)
    assert anchor in orig, anchor
    try:
        open(f'{dev}/{rel}', 'w', newline='').write(orig.replace(anchor, block + anchor, 1))
        print('==', label, flush=True)
        run(args)
    finally:
        open(f'{dev}/{rel}', 'w', newline='').write(orig)
    print('restored:', open(f'{dev}/{rel}', newline='').read() == orig, flush=True)


# 1. layout: the reset min-content test, the size test and the baseline test, on develop's layout code.
splice('crates/rustkit-layout/src/lib.rs',
       '    #[test]\n    fn a_reset_buttons_min_content_is_its_widest_word',
       '    #[test]\n    fn controls_that_cannot_wrap_have_min_content_equal_to_max_content',
       '    #[test]\n    fn controls_that_cannot_wrap_have_min_content_equal_to_max_content',
       'layout tests on develop',
       ['cargo', 'test', '-p', 'rustkit-layout', '--lib', 'button'])

# 2. engine: the default-box test on develop's engine.
splice('crates/rustkit-engine/src/lib.rs',
       '    /// The computed style of one childless element under a parent whose',
       '}\n\n#[cfg(test)]\nmod svg_image_tests',
       '}\n\n#[cfg(test)]\nmod svg_image_tests',
       'engine test on develop',
       ['cargo', 'test', '-p', 'rustkit-engine', '--lib', 'a_push_button_has_chromes_default_box'])
