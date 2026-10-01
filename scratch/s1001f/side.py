"""Put frames side by side at half size.
usage: side.py <out.png> <frame>..."""
import sys
from PIL import Image

ims = [Image.open(p).convert("RGB") for p in sys.argv[2:]]
w = sum(i.width for i in ims) // 2 + 10 * (len(ims) - 1)
h = max(i.height for i in ims) // 2
out = Image.new("RGB", (w, h), (255, 0, 255))
x = 0
for i in ims:
    out.paste(i.resize((i.width // 2, i.height // 2)), (x, 0))
    x += i.width // 2 + 10
out.save(sys.argv[1])
print(sys.argv[1], out.size)
