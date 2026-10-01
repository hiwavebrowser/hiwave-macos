"""settings: per-<select> pixel diff vs the Chrome baseline, develop binary vs fix binary.
Writes a side-by-side zoom of the first select (chrome | dev | fix)."""
import glob, json, os, subprocess, sys
from PIL import Image, ImageChops
WT = '/Users/petecopeland/Repos/.worktrees/rs-control-semantics'
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
case = sys.argv[1] if len(sys.argv) > 1 else 'settings'
base = glob.glob(f'{WT}/baselines/chrome-148/*/{case}')[0]
chrome = Image.open(f'{base}/baseline.png').convert('RGB')
W, H = chrome.size
html = {'settings': f'{WT}/crates/hiwave-app/src/ui/settings.html',
        'form-controls': f'{WT}/websuite/micro/form-controls/index.html',
        'form-elements': f'{WT}/websuite/cases/form-elements/index.html'}[case]
frames = {}
for arm, b in (('dev', 'pc-dev-9c701ab'), ('fix', 'pc-controls-wip')):
    out = f'{HUB}/scratch/s1001/{case}-{arm}.ppm'
    subprocess.run([f'{HUB}/scratch/bin/{b}', '--html-file', html, '--width', str(W), '--height', str(H),
                    '--dump-frame', out], capture_output=True, text=True, timeout=180)
    frames[arm] = Image.open(out).convert('RGB')
rects = json.load(open(f'{base}/layout-rects.json'))['elements']
sel = [e for e in rects if e['tag'] in ('select',) or (e['tag'] == 'input' and 'password' in e['selector'])]
print(case, (W, H), 'frames', {k: v.size for k, v in frames.items()}, 'selects', len(sel))


def diff(a, b, box):
    d = ImageChops.difference(a.crop(box), b.crop(box)).convert('L')
    return sum(1 for p in d.getdata() if p > 8)


for e in sel:
    r = e['rect']
    box = (int(r['x']) - 2, int(r['y']) - 2, int(r['x'] + r['width']) + 3, int(r['y'] + r['height']) + 3)
    if box[3] > H:
        continue
    print(e['selector'][-50:], [round(r[k], 1) for k in ('x', 'y', 'width', 'height')],
          'dev', diff(chrome, frames['dev'], box), 'fix', diff(chrome, frames['fix'], box))
if sel:
    r = sel[0]['rect']
    box = (int(r['x']) - 6, int(r['y']) - 6, int(r['x'] + r['width']) + 7, int(r['y'] + r['height']) + 7)
    w, h = box[2] - box[0], box[3] - box[1]
    strip = Image.new('RGB', (w * 4, h * 4 * 3))
    for i, im in enumerate((chrome, frames['dev'], frames['fix'])):
        strip.paste(im.crop(box).resize((w * 4, h * 4), Image.NEAREST), (0, i * h * 4))
    strip.save(f'{HUB}/scratch/s1001/{case}-select-strip.png')
    print('strip', strip.size)
