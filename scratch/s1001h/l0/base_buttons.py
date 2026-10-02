"""Chrome's committed baseline rects and computed styles for button-like elements, across all baseline cases.
usage: base_buttons.py [tag ...]   (default: button)"""
import glob, json, os, sys
B = '/Users/petecopeland/Repos/.worktrees/rs-dev-9f49a40/baselines/chrome-148'
tags = sys.argv[1:] or ['button']
for rp in sorted(glob.glob(f'{B}/*/*/layout-rects.json')):
    case = '/'.join(rp.split('/')[-3:-1])
    rects = json.load(open(rp))['elements']
    sp = os.path.join(os.path.dirname(rp), 'computed-styles.json')
    styles = {e['selector']: e['styles'] for e in json.load(open(sp))['elements']} if os.path.exists(sp) else {}
    hits = [e for e in rects if e['tag'] in tags]
    if not hits:
        continue
    print(f'== {case}: {len(hits)}')
    for e in hits[:4]:
        r = e['rect']
        st = styles.get(e['selector'], {})
        keep = {k: st[k] for k in st if any(w in k for w in ('padding', 'border', 'font-size', 'font-family',
                                                              'line-height', 'box-sizing', 'height', 'width'))
                and 'color' not in k and 'radius' not in k and 'style' not in k}
        print(f"  {e['selector'][-50:]:50} {r['width']:.2f} x {r['height']:.2f}  {keep}")
