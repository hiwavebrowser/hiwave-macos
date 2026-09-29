"""Print the receipt header and the cases that differ from the reference column."""
import re, sys

lines = open(sys.argv[1]).read().splitlines()
print(lines[0])
for l in lines:
    m = re.match(r'\| `([\w-]+)` \| ([\d.]+) \| ([\d.]+|nan) \|', l)
    if m and m.group(3) != 'nan' and abs(float(m.group(2)) - float(m.group(3))) > 1e-4:
        print('DIFF', l)
