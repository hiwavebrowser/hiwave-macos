"""Copy last session's tools into this session's folder, pointed at s1001f and the latest quiet board."""
S = "/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch"
for src, dst, subs in (
    ("s1001e/s0_ab.py", "s1001f/ab.py", [("scratch/s1001e/frames", "scratch/s1001f/frames")]),
    ("s1001e/vs_chrome.py", "s1001f/vs_chrome.py", [("scratch/s1001e/frames", "scratch/s1001f/frames"),
                                                    ("20261001T0907Z-quiet-dev7ae0e68", "20261001T1832Z-quiet-dev2b764be")]),
    ("s1001e/fill_body.py", "s1001f/fill_body.py", [("{HUB}/s1001e/pr-body", "{HUB}/s1001f/pr-body")]),
):
    s = open(f"{S}/{src}").read()
    for a, b in subs:
        assert a in s, (src, a)
        s = s.replace(a, b)
    open(f"{S}/{dst}", "w").write(s)
    print(dst)
