"""Real-site A/B over all 20 board sites in chunks of 5, so the two arms of a site run minutes apart.
usage: ab_all.py <tag> <binA> <binB>   -> trench/realsite/runs/<tag>-c<k>-<n>-<A|B>/"""
import subprocess, sys
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
tag, a, b = sys.argv[1:4]
# Sites that load and paint text first; the blank/JS-only ones (youtube, reddit, microsoft, instagram) last.
SITES = ['google', 'facebook', 'x', 'wikipedia', 'lyft', 'shopify', 'apple', 'github', 'netflix', 'squarespace',
         'yahoo', 'cnn', 'bing', 'walmart', 'weather', 'linkedin', 'youtube', 'reddit', 'microsoft', 'instagram']
for k in range(0, len(SITES), 4):
    r = subprocess.run(['python3', f'{HUB}/scratch/s0930/ab_board.py', f'{tag}-c{k // 4}', ','.join(SITES[k:k + 4]),
                        a, b, '1'], cwd=HUB, capture_output=True, text=True)
    print(r.stdout, r.stderr[-800:], flush=True)
