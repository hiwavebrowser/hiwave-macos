"""Pinned Chrome 148 screenshot + computed styles of a local page.
usage: python3 cshot.py <file.html> <out.png> <out.json> [props]"""
import os, subprocess, sys
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
CHROME = os.path.expanduser('~/Repos/hiwave/hiwave-macos/.browsers/chrome/mac_arm-148.0.7778.216/chrome-mac-arm64/'
                            'Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing')
env = dict(os.environ, PARITY_CHROME_PATH=CHROME)
args = [os.path.abspath(a) if i < 3 else a for i, a in enumerate(sys.argv[1:])]
r = subprocess.run(['node', 'tools/parity_oracle/shot_local.mjs'] + args, cwd=HUB, env=env,
                   capture_output=True, text=True, timeout=240)
print(r.returncode, r.stdout[-500:], r.stderr[-800:])
