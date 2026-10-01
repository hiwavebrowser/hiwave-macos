"""Capture one URL with one binary; print return code, time and the stderr tail.
usage: one.py <binary> <url> <stem>"""
import subprocess, sys, time

b, url, stem = sys.argv[1:4]
t = time.time()
try:
    r = subprocess.run([b, "--url", url, "--width", "1280", "--height", "800", "--dump-frame", stem + ".ppm",
                        "--dump-display-list", stem + ".dl.json"], capture_output=True, text=True, timeout=200)
    print("rc", r.returncode, f"{time.time()-t:.0f}s")
    print((r.stdout + r.stderr)[-1500:])
except subprocess.TimeoutExpired:
    print("timeout", f"{time.time()-t:.0f}s")
