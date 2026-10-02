"""Make s1001h/fill_body.py from s1001g's (same script, this session's directory)."""
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch'
s = open(f'{HUB}/s1001g/fill_body.py').read()
open(f'{HUB}/s1001h/fill_body.py', 'w').write(s.replace('s1001g', 's1001h'))
print('ok')
