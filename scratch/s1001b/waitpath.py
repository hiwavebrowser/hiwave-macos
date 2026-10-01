"""Wait until a path exists (or a timeout). usage: waitpath.py <path> [timeout_s]"""
import os, sys, time
p = sys.argv[1]
deadline = time.time() + float(sys.argv[2] if len(sys.argv) > 2 else 560)
while time.time() < deadline and not os.path.exists(p):
    time.sleep(5)
print('exists' if os.path.exists(p) else 'still waiting', time.strftime('%H:%M'), 'load', os.getloadavg()[0])
