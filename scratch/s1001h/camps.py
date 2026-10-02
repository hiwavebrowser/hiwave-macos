"""Run camp3.py for several arms in sequence (they share one staging path). usage: camps.py label=binary ..."""
import subprocess, sys
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch'
for spec in sys.argv[1:]:
    label, binary = spec.split('=')
    print('==', label, flush=True)
    r = subprocess.run(['python3', f'{HUB}/s1001c/camp3.py', label, binary], capture_output=True, text=True)
    print(r.stdout[-3000:], r.stderr[-800:], flush=True)
print('CAMPSDONE', flush=True)
