"""Direct children (with sample counts and source lines) of every frame matching a substring.
usage: sample_children.py <sample.txt> <substr>"""
import re, sys

t = open(sys.argv[1], errors="replace").read()
cg = t[t.find("Call graph:"): t.find("Total number in stack")]
L = cg.splitlines()
rx = re.compile(r"^([\s+!:|]*)(\d+)\s+(.+?)\s+\(in ")
for k, l in enumerate(L):
    m = rx.match(l)
    if not m or sys.argv[2] not in m.group(3):
        continue
    d = len(m.group(1))
    print("FRAME", m.group(2), l.strip()[-60:])
    child_d = None
    for l2 in L[k + 1:]:
        m2 = rx.match(l2)
        if not m2:
            continue
        d2 = len(m2.group(1))
        if d2 <= d:
            break
        if child_d is None:
            child_d = d2
        if d2 == child_d:
            name = re.sub(r"::h[0-9a-f]{16}", "", m2.group(3))[:80]
            print("   ", m2.group(2), name, "|", l2.strip()[-45:])
