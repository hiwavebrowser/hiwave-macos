"""N nested column flex-grow wrappers around an empty basis-0 grow row item."""
import os, subprocess, sys
HERE = os.path.dirname(os.path.abspath(__file__))
BIN = sys.argv[1]
item = sys.argv[2] if len(sys.argv) > 2 else 'display:flex;flex-basis:0;flex-grow:1'
blocks = []
for n in range(1, 7):
    inner = f'<div id="c{n}" style="{item}"><div class="main"></div></div>'
    for k in range(n):
        inner = f'<div class="col" style="flex-grow:1">{inner}</div>'
    blocks.append(f'<div id="a{n}" class="col">{inner}</div><hr id="h{n}">')
html = """<!doctype html><html><head><style>
body{margin:0}.col{display:flex;flex-direction:column}.main{background:#def}hr{margin:0;border:0;height:1px}
</style></head><body>""" + ''.join(blocks) + '</body></html>'
p = os.path.join(HERE, 'depth2.html')
open(p, 'w').write(html)
subprocess.run(['python3', os.path.join(HERE, 'cmp.py'), BIN, p])
