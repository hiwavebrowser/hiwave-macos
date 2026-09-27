#!/usr/bin/env python3
"""Serve scratch/resize over 127.0.0.1 in-process, run resize_check on pages.
usage: serve_and_check.py <capture-bin> <out-dir> <WxH> <page.html> [...]"""
import functools
import http.server
import os
import re
import glob
import subprocess
import sys
import threading

here = os.path.dirname(os.path.abspath(__file__))
handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=here)
handler.log_message = lambda *a: None
srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
threading.Thread(target=srv.serve_forever, daemon=True).start()
port = srv.server_address[1]
bin_, out, wxh = sys.argv[1:4]
urls = [f"http://127.0.0.1:{port}/{p}" for p in sys.argv[4:]]
subprocess.run([sys.executable, os.path.join(here, "resize_check.py"), bin_, out, wxh, *urls])
for f in sorted(glob.glob(os.path.join(out, "*.dl.json"))):
    t = open(f).read()
    print(os.path.basename(f), re.findall(r'"((?:iw|mq-|resized|no-resize|init)[^"]*)"', t))
srv.shutdown()
