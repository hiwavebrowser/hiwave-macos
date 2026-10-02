#!/usr/bin/env python3
"""wait_pid.py <pid> [limit=540]: block until the process is gone or the limit passes. Prints the load each minute."""
import os
import sys
import time

pid = int(sys.argv[1])
limit = float(sys.argv[2]) if len(sys.argv) > 2 else 540.0
start = time.time()
while True:
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        print("gone after %.0f s, load %.1f" % (time.time() - start, os.getloadavg()[0]))
        sys.exit(0)
    if time.time() - start >= limit:
        print("still running after %.0f s, load %.1f" % (limit, os.getloadavg()[0]))
        sys.exit(1)
    time.sleep(10)
    if int(time.time() - start) % 60 < 10:
        print("%s load %.1f" % (time.strftime("%H:%M:%S"), os.getloadavg()[0]), flush=True)
