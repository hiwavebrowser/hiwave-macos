"""Wait until a file is non-empty (or a timeout), then print its summary lines.
usage: python3 waitfile.py <file> [timeout_s]"""
import os, re, sys, time
f = sys.argv[1]
deadline = time.time() + float(sys.argv[2] if len(sys.argv) > 2 else 580)
while time.time() < deadline and not (os.path.exists(f) and os.path.getsize(f) > 0):
    time.sleep(5)
time.sleep(2)
if not (os.path.exists(f) and os.path.getsize(f) > 0):
    print('still running')
    sys.exit(0)
for line in open(f):
    if re.search(r'rc=|FAILED|test result|panicked', line):
        print(line.rstrip()[:300])
