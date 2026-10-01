"""Print two tiny PNGs as Rust byte-array literals: 4x4 fully transparent, 4x4 opaque red."""
import io
from PIL import Image
for name, rgba in (('CLEAR_PNG', (0, 0, 0, 0)), ('RED_PNG', (255, 0, 0, 255))):
    b = io.BytesIO()
    Image.new('RGBA', (4, 4), rgba).save(b, 'PNG', optimize=True)
    data = b.getvalue()
    rows = []
    for i in range(0, len(data), 12):
        rows.append('        ' + ', '.join(f'0x{c:02x}' for c in data[i:i + 12]) + ',')
    print(f'    const {name}: &[u8] = &[\n' + '\n'.join(rows) + '\n    ];')
