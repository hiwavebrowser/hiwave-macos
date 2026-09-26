"""Time one parity-capture --url run. usage: time_site.py <bin> <url> <tag> [timeout_ms]"""
import subprocess, sys, time

binary, url, tag = sys.argv[1], sys.argv[2], sys.argv[3]
timeout_ms = sys.argv[4] if len(sys.argv) > 4 else "90000"
t0 = time.time()
p = subprocess.run([binary, "--url", url, "--width", "1280", "--height", "800", "--timeout-ms", timeout_ms,
                    "--dump-frame", f"/tmp/{tag}.ppm", "--dump-display-list", f"/tmp/{tag}-dl.json"],
                   capture_output=True, text=True)
dt = time.time() - t0
open(f"/tmp/{tag}.err", "w").write(p.stderr)
print(f"{tag}: rc={p.returncode} wall={dt:.1f}s")
print("\n".join(p.stderr.strip().splitlines()[-4:]))
