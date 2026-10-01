"""Which python3 on this machine can import PIL."""
import glob, shutil, subprocess, sys

cands = [sys.executable, shutil.which("python3"), "/usr/bin/python3", "/opt/homebrew/bin/python3",
         "/usr/local/bin/python3"] + glob.glob("/opt/homebrew/bin/python3.*") + glob.glob(
    "/Library/Frameworks/Python.framework/Versions/*/bin/python3") + glob.glob(
    "/Users/petecopeland/.pyenv/versions/*/bin/python3")
seen = set()
for c in cands:
    if not c or c in seen or c.endswith("-config"):
        continue
    seen.add(c)
    try:
        r = subprocess.run([c, "-c", "import PIL, sys; print(PIL.__version__, sys.version.split()[0])"],
                           capture_output=True, text=True, timeout=20)
        print(c, "->", (r.stdout or r.stderr).strip().splitlines()[-1])
    except Exception as e:
        print(c, "->", e)
