"""camp3.py on several arms, one after the other. usage: camp_both.py <label>=<binary>..."""
import subprocess, sys
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
for arg in sys.argv[1:]:
    label, binary = arg.split('=')
    print('==', label, flush=True)
    r = subprocess.run(['python3', f'{HUB}/scratch/s1001c/camp3.py', label, binary], capture_output=True, text=True)
    print(r.stdout[-3000:], r.stderr[-1000:], flush=True)
print('ALL DONE', flush=True)
