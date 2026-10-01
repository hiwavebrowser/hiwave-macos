"""Write the real-site section inputs (frames-body.txt, runs-body.txt) from frames2.txt."""
D = "/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/s1001f"
rows = [l for l in open(f"{D}/frames2.txt").read().splitlines() if "within develop" in l]
frames = """RustKit-only A/B on live URLs, develop __BASE__ vs fix __HEAD__, each captured A, B, A, B at 1280x800 (machine load 12 to 23, so no Chrome oracle run; `nan` is a capture that did not finish in 150 s). "within" is the same binary twice (the site's own variance), "across" is the four develop/fix pairs:

```
""" + "\n".join(rows) + """
```

- **wikipedia** is the one site whose frame follows the binary (12.30% of pixels, 0.00% within each arm): its 2938 text commands go from the system font to Helvetica, Helvetica-Bold and Helvetica-Oblique, which is what Chrome draws.
- **facebook** moves 1.05%: five text commands with no author font are now Times-Bold.
- **x**, **lyft** and **shopify** are pixel-identical across arms. google and netflix differ within an arm by as much as across (rotating content).
- **x crashed on the first commit** (35b90b8) and loads on the head: see the third test above."""
runs = """The same frames against the Chrome captures stored by the last quiet board (`20261001T1832Z-quiet-dev2b764be`, the oracle's own `diff`), develop -> fix:

| site | Chrome capture | develop | fix |
|---|---|---|---|
| wikipedia | b | 15.15% | 15.28% |
| wikipedia | a (the unstable capture: Chrome-vs-Chrome was 42.5% in that run) | 44.24% | 44.22% |
| facebook | a and b | 13.63% | 13.55% |
| x | a | 10.39% | 10.39% |

No board check changes. wikipedia does not move toward Chrome on the pixel meter even though its face is now Chrome's: the page's diff is its layout (no sidebar, no banner, a different column), and the text sits in different places in the two browsers either way. The claim this PR makes for real sites is the face and its metrics, measured on the fixture above, not a board point."""
open(f"{D}/frames-body.txt", "w").write(frames)
open(f"{D}/runs-body.txt", "w").write(runs)
print(len(rows), "rows")
