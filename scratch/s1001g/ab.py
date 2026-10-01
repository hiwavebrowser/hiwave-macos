"""Real-site frames A/B, RustKit only, with no PIL (the PATH python3 lost it on 2026-10-01 evening).
Capture each board site A,B,A,B; print the frame difference within each arm (the site's own variance)
and across arms; keep each frame as PNG in scratch/s1001g/frames for vs_chrome.py.
usage: ab.py <binA> <binB> <site id>..."""
import itertools, json, os, struct, subprocess, sys, time, zlib

HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
OUT = f'{HUB}/scratch/s1001g/frames'
os.makedirs(OUT, exist_ok=True)
a, b = sys.argv[1:3]
j = json.load(open(f'{HUB}/websuite/realsite-top20.json'))
urls = {s['id']: s['url'] for s in (j['sites'] if isinstance(j, dict) else j)}


def read_ppm(path):
    d = open(path, 'rb').read()
    assert d[:2] == b'P6', d[:2]
    vals, i = [], 2
    while len(vals) < 3:
        while d[i:i + 1].isspace():
            i += 1
        if d[i:i + 1] == b'#':
            i = d.index(b'\n', i)
            continue
        k = i
        while not d[k:k + 1].isspace():
            k += 1
        vals.append(int(d[i:k]))
        i = k
    w, h, _ = vals
    return w, h, d[i + 1:i + 1 + w * h * 3]


def write_png(path, w, h, rgb):
    def chunk(tag, data):
        c = struct.pack('>I', len(data)) + tag + data
        return c + struct.pack('>I', zlib.crc32(tag + data) & 0xffffffff)
    raw = b''.join(b'\x00' + rgb[y * w * 3:(y + 1) * w * 3] for y in range(h))
    open(path, 'wb').write(b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 2, 0, 0, 0))
                           + chunk(b'IDAT', zlib.compress(raw, 6)) + chunk(b'IEND', b''))


def shot(binp, url, stem):
    for f in (stem + '.ppm', stem + '.png'):
        if os.path.exists(f):
            os.remove(f)
    cmd = [binp, '--url', url, '--width', '1280', '--height', '800', '--dump-frame', stem + '.ppm']
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=150)
    except subprocess.TimeoutExpired:
        return None
    if r.returncode != 0 or not os.path.exists(stem + '.ppm'):
        open(stem + '.err', 'w').write(f'rc {r.returncode}\n' + r.stderr[-4000:])
        return None
    f = read_ppm(stem + '.ppm')
    write_png(stem + '.png', *f)
    os.remove(stem + '.ppm')
    return f


def pct(x, y):
    """Share of pixels where any channel differs by more than 8 (the same count ab.py made on PIL's
    greyscale difference is close to this, not equal: this one is per channel)."""
    if x is None or y is None or x[:2] != y[:2]:
        return float('nan')
    p, q = x[2], y[2]
    if p == q:
        return 0.0
    n = 0
    for i in range(0, len(p), 3):
        if p[i:i + 3] != q[i:i + 3] and (abs(p[i] - q[i]) > 8 or abs(p[i + 1] - q[i + 1]) > 8
                                         or abs(p[i + 2] - q[i + 2]) > 8):
            n += 1
    return 100.0 * n / (x[0] * x[1])


for name in sys.argv[3:]:
    f = {}
    secs = []
    for k, binp in (('A1', a), ('B1', b), ('A2', a), ('B2', b)):
        t = time.time()
        f[k] = shot(binp, urls[name], f'{OUT}/{name}-{k}')
        secs.append(f'{time.time() - t:.0f}')
    print(f"{name:12} within develop {pct(f['A1'], f['A2']):6.2f}%  within fix {pct(f['B1'], f['B2']):6.2f}%  "
          f"across " + ' '.join(f"{pct(f[x], f[y]):6.2f}%" for x, y in itertools.product(('A1', 'A2'), ('B1', 'B2')))
          + '  failed: ' + (','.join(k for k in f if f[k] is None) or 'none')
          + '  secs A,B,A,B ' + ','.join(secs), flush=True)
