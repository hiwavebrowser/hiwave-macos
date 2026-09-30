"""Fill the nesting PR body's receipt and compare the ratchet outputs of the two arms."""
import difflib, shutil, subprocess
D = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/'
body = D + 's0930/pr-body-nest-filled.md'
shutil.copy(D + 's0930/pr-body-nest.md', body)
r = subprocess.run(['python3', D + 's0930/fill_receipt.py', 'ld-eef161e', 'nest-78c3ebd', body, 'eef161e', '78c3ebd'],
                   capture_output=True, text=True)
print(r.stdout[-500:], r.stderr[-800:])
s = open(body).read()
i = s.find('## Campaign receipt')
print(s[i:i + 1500])
a = open(D + 's0929e/camp-ld-eef161e/ratchet.txt').read()
b = open(D + 's0929e/camp-nest-78c3ebd/ratchet.txt').read()
print('RATCHET IDENTICAL' if a == b else 'RATCHET DIFFERS')
if a != b:
    print('\n'.join(list(difflib.unified_diff(a.splitlines(), b.splitlines(), lineterm=''))[:30]))
