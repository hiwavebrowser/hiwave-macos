"""Time parity-capture --url for each arm, alternating, N rounds.
usage: time_arms.py <url> <rounds> <label=bin> ..."""
import subprocess, sys, time

url, rounds = sys.argv[1], int(sys.argv[2])
arms = [a.split("=", 1) for a in sys.argv[3:]]
for r in range(rounds):
    for label, b in (arms if r % 2 == 0 else arms[::-1]):
        t = time.time()
        p = subprocess.run([b, "--url", url, "--timeout-ms", "60000"], capture_output=True, text=True)
        print(f"{url} {label} round {r}: {time.time() - t:.1f}s exit {p.returncode}", flush=True)
