"""Build the rustkit-engine lib test binary in a worktree, run it under a time limit, and report failures and,
if it was cut off, the tests that had started and not finished.
usage: engine_tests.py <worktree> <seconds> [filter]"""
import json, re, subprocess, sys, time

wt, limit = sys.argv[1], float(sys.argv[2])
flt = sys.argv[3:4]
t = time.time()
b = subprocess.run(["cargo", "test", "-p", "rustkit-engine", "--lib", "--no-run", "--message-format=json"],
                   cwd=wt, capture_output=True, text=True)
exe = None
for line in b.stdout.splitlines():
    try:
        j = json.loads(line)
    except ValueError:
        continue
    if j.get("executable") and j.get("target", {}).get("name") == "rustkit_engine" and j.get("profile", {}).get("test"):
        exe = j["executable"]
print(f"build rc={b.returncode} {time.time()-t:.0f}s exe={exe}", flush=True)
if not exe:
    print(b.stderr[-2000:])
    sys.exit(1)
log = "/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/s1001f/engine-tests.log"
t = time.time()
with open(log, "w") as f:
    p = subprocess.Popen([exe, *flt], cwd=f"{wt}/crates/rustkit-engine", stdout=f, stderr=subprocess.STDOUT)
    try:
        rc = p.wait(timeout=limit)
    except subprocess.TimeoutExpired:
        p.kill()
        rc = "timeout"
out = open(log).read()
ok = len(re.findall(r"^test .* \.\.\. ok$", out, re.M))
failed = re.findall(r"^test (.*) \.\.\. FAILED$", out, re.M)
print(f"rc={rc} {time.time()-t:.0f}s ok={ok} failed={len(failed)}")
for name in failed:
    print("FAILED", name)
for line in out.splitlines():
    if line.startswith("test result") or "has been running for over" in line:
        print(line)
