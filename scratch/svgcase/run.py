"""Capture each fixture with <bin> and print FillPolygon bounds. usage: run.py <bin> <name>..."""
import json, os, re, subprocess, sys
here = os.path.dirname(os.path.abspath(__file__))
binary = sys.argv[1]
for n in sys.argv[2:]:
    base = os.path.join(here, n)
    p = subprocess.run([binary, "--html-file", base + ".html", "--width", "200", "--height", "200",
                        "--dump-frame", base + ".ppm", "--dump-display-list", base + ".json"],
                       capture_output=True, text=True)
    if p.returncode:
        print(n, "exit", p.returncode, p.stderr[-400:])
        continue
    for c in json.load(open(base + ".json"))["commands"]:
        s = c.get("debug", "")
        if s.startswith("FillPolygon"):
            pts = [tuple(map(float, m)) for m in re.findall(r"\(([-\d.]+), ([-\d.]+)\)", s)]
            xs = [q[0] for q in pts]; ys = [q[1] for q in pts]
            print(n, round(min(xs), 1), round(max(xs), 1), round(min(ys), 1), round(max(ys), 1))
