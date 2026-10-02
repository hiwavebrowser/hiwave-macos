"""A 20x20 PNG with four distinct quadrants, so position, size and repeat are all visible."""
from PIL import Image
im = Image.new('RGB', (20, 20))
for y in range(20):
    for x in range(20):
        im.putpixel((x, y), [(200, 0, 0), (0, 150, 0), (0, 0, 200), (230, 160, 0)][(x >= 10) + 2 * (y >= 10)])
im.save('/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/s1002b/dot.png')
