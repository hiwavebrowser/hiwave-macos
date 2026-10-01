"""Failing-first helper: swap the container-side fit-content arms in flex.rs off or on.
usage: python3 toggle.py off|on"""
import sys
P = '/Users/petecopeland/Repos/.worktrees/rs-light-dark/crates/rustkit-layout/src/flex.rs'
PAIRS = [
    ('!matches!(container.style.height, Length::Auto | Length::FitContent)',
     '!matches!(container.style.height, Length::Auto /*TOGGLE1*/)'),
    ('rustkit_css::Length::Auto | rustkit_css::Length::FitContent\n        ) {',
     'rustkit_css::Length::Auto /*TOGGLE2*/\n        ) {'),
]
s = open(P).read()
for on, off in PAIRS:
    a, b = (on, off) if sys.argv[1] == 'off' else (off, on)
    assert s.count(a) == 1, (a, s.count(a))
    s = s.replace(a, b)
open(P, 'w').write(s)
print('fix is', sys.argv[1])
