"""Poll a PR's checks until none is pending or the time is up; print the final table.
usage: ci_wait.py <pr> <seconds>"""
import subprocess, sys, time

pr, limit = sys.argv[1], float(sys.argv[2])
t = time.time()
while True:
    r = subprocess.run(["gh", "pr", "checks", pr, "-R", "hiwavebrowser/hiwave-macos"], capture_output=True, text=True)
    rows = [l.split("\t")[:3] for l in r.stdout.splitlines()]
    pending = [x[0] for x in rows if len(x) > 1 and x[1] == "pending"]
    failed = [x[0] for x in rows if len(x) > 1 and x[1] == "fail"]
    if not pending or failed or time.time() - t > limit:
        break
    time.sleep(30)
for x in rows:
    print("  ".join(x))
print(f"-- {time.time()-t:.0f}s pending={pending} failed={failed}")
