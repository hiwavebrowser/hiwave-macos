"""Crop a region of one or more frames (ppm/png), stack them vertically, scale, save as png.
usage: crop.py <out.png> <x0,y0,x1,y1> <scale> <frame>..."""
import sys
from PIL import Image
out, box, scale = sys.argv[1], tuple(int(v) for v in sys.argv[2].split(',')), int(sys.argv[3])
crops = [Image.open(f).convert('RGB').crop(box) for f in sys.argv[4:]]
w, h = crops[0].size
sheet = Image.new('RGB', (w, (h + 4) * len(crops)), (255, 0, 255))
for i, c in enumerate(crops):
    sheet.paste(c, (0, i * (h + 4)))
sheet = sheet.resize((sheet.width * scale, sheet.height * scale), Image.NEAREST)
sheet.save(out)
print(out, sheet.size)
