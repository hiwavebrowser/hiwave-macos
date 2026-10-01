#!/usr/bin/env python3
"""longlines.py FILE — added lines (vs HEAD) longer than 100 columns."""
import subprocess
import sys

diff = subprocess.run(['git', 'diff', '-U0', 'HEAD', '--', sys.argv[1]], capture_output=True, text=True).stdout
n = 0
for l in diff.split('\n'):
    if l.startswith('+') and not l.startswith('+++') and len(l) > 101:
        n += 1
        print(len(l) - 1, l[:100])
print('long lines:', n)
