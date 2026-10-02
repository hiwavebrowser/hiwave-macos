#!/usr/bin/env python3
"""Does RUSTKIT_STYLE_SHARE change what the pinned sites lay out and paint?

    share_check.py <parity-capture> <outdir> [rounds]

Per site and round: flag unset, =1, =verify, dumping layout JSON and display
list and keeping each load's engine log. Prints the SHA-256 of each dump (port
masked), the per-build cascade ms, the engine's "Style share" lines, and which
loads produced equal dumps. The pages fetch images from their origins, so a
load can differ from itself: rounds give off-vs-off as the control. Ends with
the total of hits, mismatches and parent mismatches the verify loads reported.
"""
import functools
import hashlib
import http.server
import os
import re
import subprocess
import sys
import threading

SNAPSHOTS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "cascade", "snapshots")
binary, outdir = sys.argv[1:3]
rounds = int(sys.argv[3]) if len(sys.argv) > 3 else 2
os.makedirs(outdir, exist_ok=True)
ANSI = re.compile(r"\x1b\[[0-9;]*m")


class Quiet(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a):
        pass


srv = http.server.ThreadingHTTPServer(
    ("127.0.0.1", 0), functools.partial(Quiet, directory=SNAPSHOTS))
threading.Thread(target=srv.serve_forever, daemon=True).start()
port = srv.server_address[1]


def sha(path):
    if not os.path.exists(path):
        return None
    data = open(path, "rb").read().replace(str(port).encode(), b"PORT")
    return hashlib.sha256(data).hexdigest()[:16]


totals = {"hits": 0, "mismatches": 0, "parent_mismatches": 0,
          "match_hits": 0, "match_misses": 0, "match_untracked": 0, "match_mismatches": 0}
for site in ("cnn", "github", "wikipedia"):
    url = "http://127.0.0.1:%d/%s/index.html" % (port, site)
    got = {}
    for r in range(rounds):
        for mode in ("off", "on", "verify"):
            tag = "%s%d" % (mode, r + 1)
            layout = os.path.join(outdir, "%s-%s-layout.json" % (site, tag))
            dl = os.path.join(outdir, "%s-%s-dl.json" % (site, tag))
            env = dict(os.environ, RUSTKIT_CASCADE_TIMING="1",
                       RUST_LOG="warn,rustkit_engine=info", NO_COLOR="1")
            env.pop("RUSTKIT_STYLE_SHARE", None)
            if mode != "off":
                env["RUSTKIT_STYLE_SHARE"] = "1" if mode == "on" else "verify"
            p = subprocess.run([binary, "--url", url, "--width", "1280", "--height", "800",
                                "--timeout-ms", "180000", "--dump-layout", layout,
                                "--dump-display-list", dl],
                               env=env, capture_output=True, text=True, errors="replace")
            log = ANSI.sub("", p.stderr)
            open(os.path.join(outdir, "%s-%s.log" % (site, tag)), "w").write(log)
            lines = log.splitlines()
            builds = [re.search(r"cascade_ms=([\d.]+)", l).group(1)
                      for l in lines if "Cascade timing" in l]
            share = [l.split("rustkit_engine: ")[-1][:130] for l in lines
                     if "Style share" in l or "Match share" in l]
            if mode == "verify":
                for s in share:
                    for name in totals:
                        m = re.search(r"\b%s=(\d+)" % name, s)
                        totals[name] += int(m.group(1)) if m else 0
            got[tag] = (sha(layout), sha(dl))
            print(site, tag, "exit", p.returncode, got[tag], "builds", builds, share, flush=True)
    for kind, i in (("layout", 0), ("display-list", 1)):
        groups = {}
        for tag, hashes in got.items():
            groups.setdefault(hashes[i], []).append(tag)
        print("  %s %s: %d distinct -> %s" % (site, kind, len(groups), list(groups.values())),
              flush=True)
srv.shutdown()
print("verify loads: " + ", ".join("%s %d" % kv for kv in totals.items()))
