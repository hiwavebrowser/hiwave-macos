"""Serve a page's directory over HTTP, capture it with one binary, print the requests the server saw and
the capture's log lines that mention a needle; save png + display list.
usage: serve_one.py <binary> <page.html> <out-stem> [needle]"""
import functools, http.server, os, subprocess, sys, threading
from PIL import Image
b, html, stem = sys.argv[1], os.path.abspath(sys.argv[2]), sys.argv[3]
needle = sys.argv[4] if len(sys.argv) > 4 else 'image'
requests = []


class H(http.server.SimpleHTTPRequestHandler):
    def log_message(self, fmt, *a):
        requests.append(fmt % a)


srv = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(H, directory=os.path.dirname(html)))
threading.Thread(target=srv.serve_forever, daemon=True).start()
url = f'http://127.0.0.1:{srv.server_address[1]}/{os.path.basename(html)}'
r = subprocess.run([b, '--url', url, '--width', '1280', '--height', '800', '--dump-frame', stem + '.ppm',
                    '--dump-display-list', stem + '.dl.json'], capture_output=True, text=True, timeout=180)
srv.shutdown()
print('rc', r.returncode)
print('requests:', *requests, sep='\n  ')
for line in (r.stdout + r.stderr).splitlines():
    if needle.lower() in line.lower():
        print('LOG', line[:300])
Image.open(stem + '.ppm').convert('RGB').crop((0, 0, 1280, 160)).save(stem + '.png')
