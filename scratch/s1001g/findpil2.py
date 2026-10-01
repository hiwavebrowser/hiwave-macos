"""Find PIL installs and the interpreters next to them."""
import glob, os, subprocess

roots = ["/opt/homebrew/lib/python3*/site-packages/PIL",
         "/opt/homebrew/Cellar/python@3*/*/Frameworks/Python.framework/Versions/*/lib/python3*/site-packages/PIL",
         "/Users/petecopeland/Library/Python/*/lib/python/site-packages/PIL",
         "/Library/Frameworks/Python.framework/Versions/*/lib/python3*/site-packages/PIL",
         "/Library/Developer/CommandLineTools/Library/Frameworks/Python3.framework/Versions/*/lib/python3*/site-packages/PIL",
         "/Users/petecopeland/Repos/*/.venv/lib/python3*/site-packages/PIL",
         "/Users/petecopeland/Repos/*/venv/lib/python3*/site-packages/PIL",
         "/Users/petecopeland/.venv*/lib/python3*/site-packages/PIL",
         "/Users/petecopeland/.pyenv/versions/*/lib/python3*/site-packages/PIL",
         "/Users/petecopeland/miniconda3/lib/python3*/site-packages/PIL",
         "/opt/homebrew/Caskroom/miniconda/base/lib/python3*/site-packages/PIL"]
for r in roots:
    for p in glob.glob(r):
        print("PIL at", p, "mtime", int(os.path.getmtime(p)))
print("interpreters:")
for p in sorted(set(glob.glob("/opt/homebrew/bin/python3*") + glob.glob("/opt/homebrew/opt/python@3*/bin/python3*")
                    + glob.glob("/Users/petecopeland/Repos/*/.venv/bin/python3")
                    + glob.glob("/Users/petecopeland/Repos/*/venv/bin/python3"))):
    if p.endswith("-config"):
        continue
    try:
        r = subprocess.run([p, "-c", "import PIL, sys; print(PIL.__version__, sys.version.split()[0])"],
                           capture_output=True, text=True, timeout=20)
        print(" ", p, "->", (r.stdout or r.stderr).strip().splitlines()[-1])
    except Exception as e:
        print(" ", p, "->", e)
print("PATH", os.environ.get("PATH"))
