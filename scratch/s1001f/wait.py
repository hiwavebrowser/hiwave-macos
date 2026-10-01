"""Block until a file contains a marker (or a second marker), or the time is up; print the file's tail.
usage: wait.py <file> <seconds> <marker> [...]"""
import os, sys, time

path, limit, markers = sys.argv[1], float(sys.argv[2]), sys.argv[3:]
t = time.time()
hit = None
while time.time() - t < limit and hit is None:
    try:
        s = open(path).read()
    except FileNotFoundError:
        s = ""
    for m in markers:
        if m.startswith("#"):
            n, _, text = m[1:].partition(":")
            if s.count(text) >= int(n):
                hit = m
        elif m in s:
            hit = m
    if hit is None:
        time.sleep(5)
print("\n".join(s.splitlines()[-40:]))
print(f"-- waited {time.time()-t:.0f}s hit={hit} load={os.getloadavg()[0]:.1f}")
