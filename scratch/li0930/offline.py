"""Fetch linkedin.com until it serves the layered-CSS variant (assets/*.css), inline that sheet,
save scratch/li0930/li-new.html, and render it with each binary given.
usage: offline.py [fetch] <bin>..."""
import os, re, subprocess, sys, time
from PIL import Image
D = os.path.dirname(os.path.abspath(__file__))
UA = 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36'
args = sys.argv[1:]
page = f'{D}/li-new.html'
if args and args[0] == 'fetch':
    args = args[1:]
    for attempt in range(12):
        html = subprocess.run(['curl', '-s', '--compressed', '-A', UA, '-H', 'Accept-Language: en-US,en;q=0.9',
                               'https://www.linkedin.com/'], capture_output=True, text=True).stdout
        links = re.findall(r'<link[^>]+rel="stylesheet"[^>]*>', html)
        hrefs = [re.search(r'href="([^"]+)"', l).group(1) for l in links if 'href=' in l]
        print(attempt, len(html), hrefs)
        if any('/assets/' in h for h in hrefs):
            break
        time.sleep(2)
    else:
        sys.exit('never got the layered variant')
    for l in links:
        href = re.search(r'href="([^"]+)"', l).group(1).replace('&amp;', '&')
        css = subprocess.run(['curl', '-s', '--compressed', '-A', UA, href], capture_output=True, text=True).stdout
        html = html.replace(l, '<style>' + css.replace('</style', '<\\/style') + '</style>')
    # page scripts are not what this measures; drop them
    html = re.sub(r'<script\b.*?</script>', '', html, flags=re.S)
    open(page, 'w').write(html)
    print('saved', len(html))
for b in args:
    tag = os.path.basename(b)
    out = f'{D}/off-{tag}'
    r = subprocess.run([b, '--html-file', page, '--dump-frame', out + '.ppm', '--dump-display-list', out + '.json'],
                       capture_output=True, text=True, timeout=180)
    print(tag, 'rc', r.returncode, r.stderr[-300:] if r.returncode else '')
    Image.open(out + '.ppm').save(out + '.png')
