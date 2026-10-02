"""Fill the last placeholders, append this session's digest section and PLAN note (once), commit the session's
hub artifacts and push atlas/trench-realsite. usage: hubcommit.py <commit-message-file>"""
import glob, subprocess, sys

HUB = "/Users/petecopeland/Repos/.worktrees/trench-realsite"
S = f"{HUB}/scratch/s1002b"
msg = sys.argv[1]
FETCH = ("@ 96fb04d: `load_images` also fetches the http(s) `url()` backgrounds the display list paints; one engine "
         "test (the image is requested and cached; a `display: none` box asks for nothing), which passes on the "
         "branch. No fail-first run, no release build, no campaign, no A/B. It is not a PR for a second reason: "
         "decision 2.")
FETCHNOTE = ("It is at 96fb04d and its test passes (`python3 scratch/s1002a/cerr.py <worktree> -p rustkit-engine "
             "--lib --features headless background_image_fetch`; the module needs the `headless` feature).")
fills = {f"{S}/digest.md": (("__FETCH__", FETCH),
                            ("- **At 05:10:** opened, no CI result or review yet.",
                             "- **At 05:16:** CI all green at f7f82fc (`unit-suites`, the full engine suite, "
                             "included), R1 DESIGN CLEAR; no R2 stamp yet; not merged.")),
         f"{S}/plan-note.md": (("__FETCHNOTE__", FETCHNOTE),)}
for path, subs in fills.items():
    s = open(path).read()
    for key, val in subs:
        if key in s:
            s = s.replace(key, val)
    assert "__FETCH" not in s
    open(path, "w").write(s)
for target, src, marker in ((f"{HUB}/trench/digest-realsite.md", f"{S}/digest.md", "## 2026-10-02 05:25"),
                            (f"{HUB}/trench/PLAN-realsite.md", f"{S}/plan-note.md", "**2026-10-02 05:25:")):
    cur = open(target).read()
    if marker in cur:
        print("already appended:", target)
        continue
    add = open(src).read()
    open(target, "w").write(cur.rstrip("\n") + "\n" + add)
    print("appended", len(add), "chars to", target)
paths = ["trench/digest-realsite.md", "trench/PLAN-realsite.md"]
for pat in ("scratch/s1002b/*.py", "scratch/s1002b/*.md", "scratch/s1002b/*.txt", "scratch/s1002b/*.html",
            "scratch/s1002b/*.chrome.json", "scratch/s1002b/*.chrome.png", "scratch/s1002b/dot.png",
            "scratch/s1002b/bgsh3-crop.png", "scratch/s1002b/google-b1b2.png", "scratch/s1002b/card-grid-ab.png",
            "scratch/s0929e/camp-dev-2dd7680/*.json", "scratch/s0929e/camp-dev-2dd7680/ratchet.txt",
            "scratch/s0929e/camp-bg-8f78611/*.json", "scratch/s0929e/camp-bg-8f78611/ratchet.txt",
            "scratch/s0929e/camp-bgl-f7f82fc/*.json", "scratch/s0929e/camp-bgl-f7f82fc/ratchet.txt"):
    paths += [q[len(HUB) + 1:] for q in glob.glob(f"{HUB}/{pat}")]
for cmd in (["git", "add", "--"] + paths,
            ["git", "commit", "-q", "-F", msg],
            ["git", "push", "-q", "origin", "atlas/trench-realsite"],
            ["git", "log", "--oneline", "-1"],
            ["git", "status", "-sb", "--untracked-files=no"]):
    r = subprocess.run(cmd, cwd=HUB, capture_output=True, text=True)
    print(cmd[1], r.returncode, (r.stdout + r.stderr).strip()[-300:])
