"""Greedy: drop each CSS declaration of fb2.html in turn; keep the drop if RustKit still
disagrees with Chrome on #hr's y (Chrome re-measured each time)."""
import json, os, re, subprocess, sys
HERE = os.path.dirname(os.path.abspath(__file__))
BIN = sys.argv[1]
CH = os.path.expanduser('~/Repos/hiwave/hiwave-macos/.browsers/chrome/mac_arm-148.0.7778.216/chrome-mac-arm64/'
                        'Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing')
src = open(os.path.join(HERE, 'fb2.html')).read()
m = re.search(r'<style>(.*?)</style>', src, re.S)
rules = re.findall(r'([^{}]+)\{([^{}]*)\}', m.group(1))
rules = [[s.strip(), [d for d in b.split(';') if d.strip()]] for s, b in rules]


def render(rs):
    css = '\n'.join(f"{s}{{{';'.join(ds)}}}" for s, ds in rs)
    return src[:m.start(1)] + css + src[m.end(1):]


def bad(html):
    p = os.path.join(HERE, 'red.html')
    open(p, 'w').write(html)
    subprocess.run(['node', os.path.join(HERE, '../../tools/parity_oracle/rects_local.mjs'), p, p + '.c.json'],
                   env=dict(os.environ, PARITY_CHROME_PATH=CH), check=True)
    subprocess.run([BIN, '--html-file', p, '--dump-layout', p + '.json'], capture_output=True)
    cy = json.load(open(p + '.c.json'))['hr'][1]
    ry = [None]

    def walk(n):
        if (n.get('selector') or '').endswith('#hr'):
            ry[0] = n['border_box']['y']
        for c in n.get('children', []):
            walk(c)
    walk(json.load(open(p + '.json'))['root'])
    return ry[0] is None or abs(ry[0] - cy) > 1, ry[0], cy


print('start', bad(render(rules)), flush=True)
for r in rules:
    if r[0] in ('.main', 'hr'):
        continue
    for d in list(r[1]):
        r[1].remove(d)
        b, ry, cy = bad(render(rules))
        if not b:
            r[1].append(d)
            print('needed', r[0], d, ry, cy, flush=True)
out = render(rules)
open(os.path.join(HERE, 'fb2-min2.html'), 'w').write(out)
print(re.search(r'<style>(.*?)</style>', out, re.S).group(1))
print('final', bad(out))
