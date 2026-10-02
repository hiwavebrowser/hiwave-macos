"""CSS background images in a live page's display list, and what became of each: capture a board
site with one binary, list `background_image` commands by url kind, and count the image-load lines
in the binary's log.
usage: bg_census.py <binary-name> <site id>..."""
import json, os, re, subprocess, sys
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
OUT = f'{HUB}/scratch/s1002e/census'
os.makedirs(OUT, exist_ok=True)
j = json.load(open(f'{HUB}/websuite/realsite-top20.json'))
urls = {s['id']: s['url'] for s in (j['sites'] if isinstance(j, dict) else j)}
binary = sys.argv[1]
for site in sys.argv[2:]:
    stem = f'{OUT}/{site}'
    try:
        r = subprocess.run([f'{HUB}/scratch/bin/{binary}', '--url', urls[site], '--width', '1280', '--height', '800',
                            '--dump-frame', stem + '.ppm', '--dump-display-list', stem + '.dl.json'],
                           capture_output=True, text=True, timeout=120)
    except subprocess.TimeoutExpired:
        print(site, 'timeout'); continue
    log = r.stdout + r.stderr
    open(stem + '.log', 'w').write(log)
    if r.returncode or not os.path.exists(stem + '.dl.json'):
        print(site, 'rc', r.returncode, log[-200:].replace('\n', ' ')); continue
    d = json.load(open(stem + '.dl.json'))
    cmds = d.get('commands') if isinstance(d, dict) else d
    kinds, first = {}, {}
    imgs = 0
    for c in cmds:
        if c.get('op') == 'image':
            imgs += 1
        if c.get('op') != 'background_image':
            continue
        u = c.get('url', '')
        rect = c.get('rect') or {}
        in_view = rect.get('y', 0) < 800 and rect.get('y', 0) + rect.get('height', 0) > 0
        if u.startswith('data:image/svg'):
            k = 'data-svg'
        elif u.startswith('data:'):
            k = 'data-raster'
        elif not re.match(r'https?://', u):
            k = 'not-absolute'
        elif re.sub(r'[?#].*', '', u).lower().endswith('.svg'):
            k = 'http-svg'
        else:
            k = 'http-raster'
        kinds.setdefault(k, [0, 0])
        kinds[k][0] += 1
        kinds[k][1] += 1 if in_view else 0
        first.setdefault(k, u[:110])
    print(f'{site:11} commands {len(cmds):5}  <img>/svg image ops {imgs:3}  background_image ops:',
          ', '.join(f'{k} {n} ({v} in first viewport)' for k, (n, v) in sorted(kinds.items())) or 'none')
    for k, u in sorted(first.items()):
        print(f'{"":13}{k}: {u}')
    n_fail = len(re.findall(r'Failed to (?:load|fetch|decode) image', log))
    n_bg = len(re.findall(r'Discovered background image', log))
    print(f'{"":13}log: image failures {n_fail}')
