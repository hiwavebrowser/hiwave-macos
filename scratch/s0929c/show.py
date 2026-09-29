"""Show an element's ancestors/children with RustKit vs Chrome rects and class decls.
usage: python3 show.py rk19 [depth]"""
import json, os, re, sys
HERE = os.path.dirname(os.path.abspath(__file__))
C = json.load(open(os.path.join(HERE, 'fb-chrome.json')))
L = json.load(open(os.path.join(HERE, 'fb-rk.json')))
tagged = open(os.path.join(HERE, 'fb-ids.html')).read()
css = open(os.path.join(HERE, '../bing0929/fb.css'), encoding='utf-8', errors='replace').read()
decl = {}
for m in re.finditer(r'\.(x[a-z0-9]+)\{([^{}]*)\}', css):
    decl.setdefault(m.group(1), m.group(2))
target, depth = sys.argv[1], int(sys.argv[2]) if len(sys.argv) > 2 else 1


def ident(nd):
    m = re.search(r'#([\w-]+)$', nd.get('selector') or '')
    return m.group(1) if m else None


def info(i, nd, ind):
    b = nd['border_box']
    c = C.get(i, [0, 0, 0, 0, '?'])
    t = re.search(r'<([a-z0-9]+)[^>]*\bid="%s"[^>]*>' % re.escape(i), tagged)
    cls = re.search(r'class="([^"]*)"', t.group(0)) if t else None
    st = re.search(r'style="([^"]*)"', t.group(0)) if t else None
    ds = [decl[k] for k in (cls.group(1).split() if cls else []) if k in decl]
    print(f"{'  '*ind}{i} <{t.group(1) if t else '?'}> rk y={b['y']:.0f} h={b['height']:.0f} w={b['width']:.0f} | ch y={c[1]:.0f} h={c[3]:.0f} w={c[2]:.0f} {c[4]}")
    print(f"{'  '*ind}   {'; '.join(ds)}" + (f" STYLE {st.group(1)}" if st else ''))


path = []


def find(nd, anc):
    i = ident(nd) if nd.get('tag') else None
    a2 = anc + [(i, nd)] if i else anc
    if i == target:
        return a2, nd
    for c in nd.get('children', []):
        r = find(c, a2)
        if r:
            return r
    return None


anc, node = find(L['root'], [])
print('--- ancestors')
for k, (i, nd) in enumerate(anc[-8:-1]):
    info(i, nd, 0)
print('--- target + descendants')


def down(nd, d, ind):
    i = ident(nd) if nd.get('tag') else None
    if i:
        info(i, nd, ind)
        if d == 0:
            return
        d -= 1
        ind += 1
    for c in nd.get('children', []):
        down(c, d, ind)


down(node, depth, 0)
