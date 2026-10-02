"""Copy the A/B frames of s1001g/frames to s1001h/<dir> (ab.py overwrites them on its next run), and print
the bounding box of the pixels that differ between A1 and B1 for the named sites (PIL).
usage: keep_frames.py <dir> [site ...]"""
import glob, os, shutil, sys
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch'
dst = f'{HUB}/s1001h/{sys.argv[1]}'
os.makedirs(dst, exist_ok=True)
n = 0
for p in glob.glob(f'{HUB}/s1001g/frames/*.png'):
    shutil.copy2(p, dst)
    n += 1
print('copied', n)
if sys.argv[2:]:
    from PIL import Image, ImageChops
    for site in sys.argv[2:]:
        a = Image.open(f'{dst}/{site}-A1.png').convert('RGB')
        b = Image.open(f'{dst}/{site}-B1.png').convert('RGB')
        d = ImageChops.difference(a, b).convert('L').point(lambda v: 255 if v > 8 else 0)
        box = d.getbbox()
        print(site, 'differs in', box)
        if box:
            x0, y0, x1, y1 = box
            pad = 6
            crop = (max(x0 - pad, 0), max(y0 - pad, 0), min(x1 + pad, a.width), min(y1 + pad, a.height))
            w, h = crop[2] - crop[0], crop[3] - crop[1]
            out = Image.new('RGB', (w, h * 2 + 4), 'red')
            out.paste(a.crop(crop), (0, 0))
            out.paste(b.crop(crop), (0, h + 4))
            out.save(f'{dst}/{site}-where.png')
            print('  ->', f'{dst}/{site}-where.png', '(develop above, fix below)', w, 'x', h)
