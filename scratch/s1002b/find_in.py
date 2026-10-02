"""Lines of a text file that contain any needle (case-folded), with line numbers, cut to 330 chars.
usage: find_in.py <file> <needle>..."""
import sys
needles = [n.lower() for n in sys.argv[2:]]
for i, line in enumerate(open(sys.argv[1], errors='replace'), 1):
    low = line.lower()
    if any(n in low for n in needles):
        print(i, line.strip()[:330])
