"""Build frames.txt (the PR's real-site A/B section) from the s0_ab.py output.
usage: mk_frames.py <ab-output> <vs-chrome.txt>"""
import sys
rows = [l.rstrip() for l in open(sys.argv[1]) if ' within develop ' in l]
note = open(sys.argv[2]).read().strip()
done = len(rows)
out = [
    f'RustKit frames only, no Chrome: each site captured develop, fix, develop, fix at 1280x800 '
    f'(`scratch/s1001e/s0_ab.py` on the hub). "within" is the site\'s own variance between two captures by the same '
    f'binary; "across" is the four develop-fix pairs. The last column is from the fix arm\'s display list. '
    f'All {done} board sites were attempted; `nan` is a capture that failed.',
    '',
    '```',
] + rows + [
    '```',
    '',
    note,
]
open('/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/s1001e/frames.txt', 'w').write('\n'.join(out) + '\n')
print(done, 'rows')
