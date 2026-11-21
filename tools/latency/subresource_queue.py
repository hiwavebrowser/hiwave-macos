"""Compare parity-capture binaries on loopback CSS and font request queues.

    python tools/latency/subresource_queue.py BEFORE AFTER

Each page has two delayed responses, the first in the queue and the first
outside its concurrency limit. Report medians after one warmup per binary.
"""

import argparse
import json
from collections import Counter, defaultdict
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from statistics import median
import subprocess
import threading
import time


class FixtureServer(ThreadingHTTPServer):
    request_queue_size = 64


class Fixture(BaseHTTPRequestHandler):
    def do_GET(self):
        site = self.server
        if self.path == "/":
            site.started = time.monotonic()
            body = site.page
            mime = "text/html"
        elif self.path in site.resources:
            index = site.resources[self.path]
            with site.lock:
                site.requests[index] += 1
                if index == site.limit:
                    site.next_request_ms = (time.monotonic() - site.started) * 1000
            time.sleep(0.3 if index in (0, site.limit) else 0.005)
            if site.kind == "css":
                body, mime = f"p {{ z-index: {index}; }}".encode(), "text/css"
            else:
                body, mime = site.font, "font/ttf"
        else:
            self.send_error(404)
            return
        self.send_response(200)
        self.send_header("Content-Type", mime)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *args):
        pass


def measure(kind, binaries, runs):
    limit = 6 if kind == "css" else 16
    with FixtureServer(("127.0.0.1", 0), Fixture) as site:
        site.kind, site.limit, site.lock = kind, limit, threading.Lock()
        paths = [f"/{i}.{kind}" for i in range(limit * 2)]
        if kind == "css":
            head = "".join(f'<link rel="stylesheet" href="{p}">' for p in paths)
        else:
            head = "<style>" + "".join(
                f"@font-face {{ font-family: f{i}; src: url({p}); }}" for i, p in enumerate(paths)
            ) + "</style>"
            site.font = (Path(__file__).resolve().parents[2] /
                         "crates/rustkit-text/tests/fixtures/Ahem.ttf").read_bytes()
        site.page = f"<!doctype html><head>{head}</head><body><p>Queue</p></body>".encode()
        site.resources = {p: i for i, p in enumerate(paths)}
        thread = threading.Thread(target=site.serve_forever, daemon=True)
        thread.start()
        results = defaultdict(list)
        try:
            for step in range(runs + 1):
                for binary in (binaries if step % 2 == 0 else reversed(binaries)):
                    site.requests = Counter()
                    capture = subprocess.run(
                        [binary, "--url", f"http://127.0.0.1:{site.server_port}/"],
                        capture_output=True, timeout=60, check=True,
                    )
                    report = json.loads(capture.stdout)
                    if report["status"] != "ok" or site.requests != Counter(range(len(paths))):
                        raise RuntimeError(f"{kind}: {report['status']}, requests {site.requests}")
                    if step:
                        results[binary].append((site.next_request_ms, report["elapsed_ms"]))
        finally:
            site.shutdown()
            thread.join()
    for binary, samples in results.items():
        next_ms, capture_ms = map(median, zip(*samples))
        print(f"{kind} {binary}: request {limit + 1} at {next_ms:.1f} ms, capture {capture_ms:g} ms")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binaries", nargs="+", type=Path)
    parser.add_argument("--runs", type=int, default=5)
    args = parser.parse_args()
    if args.runs < 1:
        parser.error("--runs must be positive")
    for kind in ("css", "font"):
        measure(kind, [p.resolve() for p in args.binaries], args.runs)
