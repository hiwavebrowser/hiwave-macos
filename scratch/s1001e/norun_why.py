"""Why does a text command have no run? Classify every run-less text op in fx/*.dl.json.
usage: norun_why.py"""
import collections, glob, json
FX = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/s1001e/fx'


def is_emoji(c):
    c = ord(c)
    return (0x1F300 <= c <= 0x1FAFF or 0x1F000 <= c <= 0x1F0FF or 0x2600 <= c <= 0x27BF
            or 0x2B00 <= c <= 0x2BFF or 0x1F1E6 <= c <= 0x1F1FF)


why = collections.Counter()
other = []
for f in sorted(glob.glob(FX + '/*.dl.json')):
    j = json.load(open(f))
    cmds = j.get('commands') if isinstance(j, dict) else j
    for c in cmds:
        if c.get('op') != 'text' or c.get('run'):
            continue
        t = c.get('text', '')
        if any(is_emoji(ch) for ch in t):
            why['has an emoji'] += 1
        elif c.get('advances') is None:
            why['legacy command (no advances either)'] += 1
            other.append((f.split('/')[-1], t[:30]))
        elif any(ord(ch) > 0x24F for ch in t):
            why['has a character outside Latin (fallback face)'] += 1
            other.append((f.split('/')[-1], t[:30]))
        else:
            why['OTHER'] += 1
            other.append((f.split('/')[-1], t[:30]))
print(dict(why))
for o in other[:30]:
    print('  ', o)
