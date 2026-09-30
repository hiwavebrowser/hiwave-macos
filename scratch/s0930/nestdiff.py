"""Pixel diff (board rule: any channel > 8/255) of offline linkedin frames vs Chrome's.
usage: python3 nestdiff.py <png>..."""
import sys
from PIL import Image, ImageChops
D = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/li0930'
chrome = Image.open(f'{D}/chrome-new.png').convert('RGB')
W, H = chrome.size
for p in sys.argv[1:]:
    im = Image.open(p).convert('RGB').crop((0, 0, W, H))
    d = ImageChops.difference(chrome, im).point(lambda v: 255 if v > 8 else 0)
    bands = d.split()
    m = ImageChops.lighter(ImageChops.lighter(bands[0], bands[1]), bands[2])
    n = sum(1 for v in m.getdata() if v)
    print(f'{p.split("/")[-1]}: {100 * n / (W * H):.1f}% differ ({W}x{H})')
