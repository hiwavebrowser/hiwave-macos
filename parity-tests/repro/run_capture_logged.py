#!/usr/bin/env python3
"""Run parity-capture with debug logs, print stylesheet parse lines."""
import subprocess
import sys
import os

html = sys.argv[1]
env = dict(os.environ)
env['RUST_LOG'] = 'rustkit_engine=debug,rustkit_css=debug'
cmd = [
    './target/release/parity-capture',
    '--html-file', html,
    '--width', '600', '--height', '500',
]
r = subprocess.run(cmd, capture_output=True, text=True, timeout=120, env=env)
for line in (r.stdout + r.stderr).splitlines():
    if any(k in line for k in ('Parsed stylesheet', 'Failed to parse', 'CSS parsed', 'Parsing CSS', 'Extracted stylesheets')):
        print(line)
