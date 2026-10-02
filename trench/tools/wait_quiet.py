#!/usr/bin/env python3
"""Block until the 1-minute load average is below <load>, or <limit> seconds pass.

    wait_quiet.py [load=5.0] [limit=540]

A timing read above load ~6 does not count (see ab2.py); this waits for the
other lane's build to finish instead of timing through it. Exit 0 when quiet,
1 when the limit passed. Prints the load every minute.
"""
import os
import sys
import time

want = float(sys.argv[1]) if len(sys.argv) > 1 else 5.0
limit = float(sys.argv[2]) if len(sys.argv) > 2 else 540.0
start = time.time()
while True:
    load = os.getloadavg()[0]
    waited = time.time() - start
    if load < want:
        print(f"quiet {time.strftime('%H:%M:%S')} load {load:.1f} after {waited:.0f} s")
        sys.exit(0)
    if waited >= limit:
        print(f"not quiet after {limit:.0f} s {time.strftime('%H:%M:%S')} load {load:.1f}")
        sys.exit(1)
    print(f"{time.strftime('%H:%M:%S')} load {load:.1f}", flush=True)
    time.sleep(min(60, max(1, limit - waited)))
