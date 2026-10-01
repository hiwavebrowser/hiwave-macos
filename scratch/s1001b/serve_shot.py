"""Serve a directory over local HTTP, capture one page with a parity-capture binary via --url, write
png + 2x crop + display list, and print the font families / widths of every text op.
usage: serve_shot.py <binary> <dir> <page> <out-stem> [crop_w crop_h]"""
import functools, http.server, json, os, subprocess, sys, threading
from PIL import Image

b, root, page, stem = sys.argv[1:5]
w, h = (int(sys.argv[5]), int(sys.argv[6])) if len(sys.argv) > 6 else (640, 200)
requests = []


class H(http.server.SimpleHTTPRequestHandler):
    def log_message(self, fmt, *a):
        requests.append(fmt % a)


srv = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(H, directory=root))
port = srv.server_address[1]
threading.Thread(target=srv.serve_forever, daemon=True).start()
r = subprocess.run([b, '--url', f'http://127.0.0.1:{port}/{page}', '--width', '1280', '--height', '800',
                    '--dump-frame', stem + '.ppm', '--dump-display-list', stem + '.dl.json'],
                   capture_output=True, text=True, timeout=180)
srv.shutdown()
print('requests:', *requests, sep='\n  ')
if r.returncode:
    print(r.stdout[-1500:], r.stderr[-1500:])
    sys.exit(r.returncode)
for line in (r.stdout + r.stderr).splitlines():
    if 'font' in line.lower():
        print('LOG', line[:240])
im = Image.open(stem + '.ppm').convert('RGB')
im.save(stem + '.png')
im.crop((0, 0, w, h)).resize((w * 2, h * 2), Image.NEAREST).save(stem + '-zoom.png')


def walk(o):
    if isinstance(o, dict):
        if any(k in o for k in ('text', 'Text')) or o.get('type') == 'text':
            yield o
        for v in o.values():
            yield from walk(v)
    elif isinstance(o, list):
        for v in o:
            yield from walk(v)


for op in walk(json.load(open(stem + '.dl.json'))):
    print(json.dumps(op)[:260])
print('->', stem + '.png', im.size)
