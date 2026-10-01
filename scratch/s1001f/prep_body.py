"""Fill __GENERIC__ and __FAILFIRST__ in s1001f/pr-body.md (in place) from this session's measurement files."""
D = "/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/s1001f"


def rows(path):
    out = {}
    for line in open(path).read().splitlines():
        p = line.split()
        if len(p) >= 5 and p[0] not in ("id", "sample"):
            out[p[0]] = (p[1], p[2], p[4])
    return out


dev, fix = rows(f"{D}/generic-dev.txt"), rows(f"{D}/generic-fix.txt")
css = {"unstyled": "(no font-family)", "sans": "sans-serif", "serif": "serif", "mono": "monospace",
       "nsans": '"No Such Family", sans-serif', "nserif": '"No Such Family", serif',
       "nmono": '"No Such Family", monospace', "none": '"No Such Family"', "helv": "Helvetica", "times": "Times",
       "tnr": '"Times New Roman"', "menlo": "Menlo", "sys": "system-ui", "cursive": "cursive", "fantasy": "fantasy"}
lines = [f'{"font-family":30} {"Chrome":>8} {"develop":>8} {"fix":>8}  develop face -> fix face']
for k, name in css.items():
    c, d, face_d = dev[k]
    _, f, face_f = fix[k]
    lines.append(f"{name:30} {c:>8} {d:>8} {f:>8}  {face_d} -> {face_f}" if face_d != face_f
                 else f"{name:30} {c:>8} {d:>8} {f:>8}  {face_f}")
ff = [l for l in open(f"{D}/failfirst.txt").read().splitlines() if not l.startswith("error")]
s = open(f"{D}/pr-body.md").read()
s = s.replace("__GENERIC__", "\n".join(lines)).replace("__FAILFIRST__", "\n".join(ff))
open(f"{D}/pr-body.md", "w").write(s)
print("\n".join(lines))
