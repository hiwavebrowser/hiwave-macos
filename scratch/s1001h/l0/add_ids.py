"""Give every span and button inside an item an id (<item>-s<n> / <item>-b), so shot_local.mjs reports them."""
import re
P = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/s1001h/l0/l0-grid-flex.html'
out = []
for line in open(P):
    m = re.search(r'id="(\w+)"', line)
    if m and 'class="item' in line:
        item, n = m.group(1), [0]

        def span(_):
            n[0] += 1
            return f'<span id="{item}-s{n[0]}">'
        line = re.sub(r'<span>', span, line)
        line = line.replace('<button>', f'<button id="{item}-b">')
    out.append(line)
open(P, 'w').write(''.join(out))
print('ok')
