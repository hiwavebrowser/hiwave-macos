"""Turn box_cmp.py's raw rows for bgsh3.html into the PR's table: the declaration with the data: url
shortened to IMG and the blue gradient to GRAD. -> fixture.txt"""
import re
D = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/s1002b'
rules = {}
for line in open(f'{D}/bgsh3.html'):
    m = re.match(r'#(f\d+) \{ (.*) \}', line.strip())
    if m:
        r = re.sub(r'url\(data:[^)]*\)', 'IMG', m.group(2)).replace('linear-gradient(blue, blue)', 'GRAD')
        rules[m.group(1)] = r.strip().rstrip(';')
rows = []
tail = ''
for line in open(f'{D}/fixture-raw.txt'):
    m = re.match(r'(f\d+)\s+develop\s+([\d.]+)%\s+base433\s+([\d.]+)%\s+fix\s+([\d.]+)%', line)
    if m:
        rows.append((rules[m.group(1)], m.group(2), m.group(3), m.group(4)))
    elif line.startswith('boxes more'):
        n = re.findall(r'(\d+ of \d+)', line)
        tail = n
w = max(len(r[0]) for r in rows)
out = [f'{"declaration (IMG = the data: PNG, GRAD = linear-gradient(blue, blue))":{w}}  before #433  develop 33d7368  fix']
for r in rows:
    out.append(f'{r[0]:{w}}  {r[1] + "%":>11}  {r[2] + "%":>15}  {r[3] + "%":>7}')
out.append('')
out.append(f'{"boxes more than 1% off Chrome":{w}}  {tail[0]:>11}  {tail[1]:>15}  {tail[2]:>7}')
open(f'{D}/fixture.txt', 'w').write('\n'.join(out) + '\n')
print('\n'.join(out))
