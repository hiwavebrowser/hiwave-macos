"""Wait until a log file contains a line starting with 'rc=' (or a timeout), then print the file.
usage: python3 waitrc.py <file> [timeout_s=560]"""
import os, sys, time
f = sys.argv[1]
deadline = time.time() + float(sys.argv[2] if len(sys.argv) > 2 else 560)


def done():
    return os.path.exists(f) and any(l.startswith('rc=') or ' rc=' in l for l in open(f, errors='replace'))


while time.time() < deadline and not done():
    time.sleep(5)
print(open(f, errors='replace').read()[-6000:] if os.path.exists(f) else 'no file')
print('DONE' if done() else 'STILL RUNNING', f'load={os.getloadavg()[0]:.1f}')
