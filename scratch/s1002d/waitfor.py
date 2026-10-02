"""Wait (foreground) until a file exists or N seconds pass. usage: waitfor.py <path> <max seconds>"""
import os, sys, time
path, limit = sys.argv[1], float(sys.argv[2])
t = time.time()
while not os.path.exists(path) and time.time() - t < limit:
    time.sleep(5)
print('exists' if os.path.exists(path) else 'not yet', f'{time.time()-t:.0f}s load={os.getloadavg()[0]:.1f}')
