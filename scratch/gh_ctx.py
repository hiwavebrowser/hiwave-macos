"""Print context around a needle in one of github.com's stylesheets.
usage: gh_ctx.py <sheet-substring> <needle> [before] [after]"""
import re, sys, urllib.request

ua = {'User-Agent': 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Safari/605.1.15'}
sheet, needle = sys.argv[1], sys.argv[2]
before = int(sys.argv[3]) if len(sys.argv) > 3 else 1500
after = int(sys.argv[4]) if len(sys.argv) > 4 else 100
html = urllib.request.urlopen(urllib.request.Request('https://github.com/', headers=ua), timeout=20).read().decode('utf8', 'replace')
link = [x for x in re.findall(r'<link[^>]+href="([^"]+\.css)"', html) if sheet in x][0]
css = urllib.request.urlopen(urllib.request.Request(link, headers=ua), timeout=20).read().decode('utf8', 'replace')
open('/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/ecp/' + sheet + '.css', 'w').write(css)
i = css.find(needle)
print(css[max(0, i - before):i + after])
