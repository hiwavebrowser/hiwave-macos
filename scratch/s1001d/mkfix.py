"""Make the fixtures for rounded shadows and rounded clipping of images and glyphs."""
import os
from PIL import Image, ImageDraw
D = os.path.dirname(os.path.abspath(__file__))
# An opaque photo-like tile: quadrants of four colours, so a corner that should be cut is easy to see.
im = Image.new('RGBA', (120, 120), (0, 0, 0, 255))
d = ImageDraw.Draw(im)
d.rectangle((0, 0, 59, 59), fill=(220, 40, 40, 255))
d.rectangle((60, 0, 119, 59), fill=(40, 160, 60, 255))
d.rectangle((0, 60, 59, 119), fill=(40, 80, 220, 255))
d.rectangle((60, 60, 119, 119), fill=(240, 200, 40, 255))
im.save(D + '/quad.png')
# A logo-like tile with a transparent surround: an opaque disc on alpha 0, plus a half-transparent band.
im = Image.new('RGBA', (120, 120), (0, 0, 0, 0))
d = ImageDraw.Draw(im)
d.ellipse((20, 20, 99, 99), fill=(200, 30, 120, 255))
d.rectangle((0, 100, 119, 119), fill=(255, 255, 255, 128))
im.save(D + '/alpha.png')
print('ok')
