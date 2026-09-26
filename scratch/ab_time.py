"""A/B wall time + frame identity. usage: ab_time.py <binA> <binB> <url> <tag>"""
import hashlib, subprocess, sys, time

a, b, url, tag = sys.argv[1:5]
res = {}
for arm, binary in (("a", a), ("b", b)):
    t0 = time.time()
    p = subprocess.run([binary, "--url", url, "--width", "1280", "--height", "800", "--timeout-ms", "90000",
                        "--dump-frame", f"/tmp/{tag}-{arm}.ppm"], capture_output=True, text=True)
    dt = time.time() - t0
    try:
        h = hashlib.sha1(open(f"/tmp/{tag}-{arm}.ppm", "rb").read()).hexdigest()[:10]
    except OSError:
        h = "none"
    res[arm] = h
    print(f"{tag} {arm}: rc={p.returncode} wall={dt:.1f}s frame={h}", flush=True)
print(f"{tag}: frames {'IDENTICAL' if res['a'] == res['b'] else 'differ'}")
