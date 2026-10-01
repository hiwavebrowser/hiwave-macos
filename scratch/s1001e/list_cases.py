"""Print the html path of every campaign case (builtins + websuite + micro), one per line.
usage: list_cases.py <repo>"""
import os, sys
repo = sys.argv[1]
sys.path.insert(0, repo + '/scripts')
from parity_lib import _cases_for_scope  # noqa: E402
seen = set()
for scope in ('builtins', 'websuite', 'micro'):
    for c in _cases_for_scope(scope):
        p = os.path.join(repo, c[1])
        if p not in seen and os.path.exists(p):
            seen.add(p)
            print(p)
