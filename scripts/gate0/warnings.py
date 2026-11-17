#!/usr/bin/env python3
"""Gate 0's warning census: the compiler warnings in `cargo check` logs, as a set, against a baseline.

Gate 0 compiles the tree after every change and every batch merge, and a warning that is not in the
baseline stops it. The set is the one the batch counts:

  engine + app:  cargo check -p rustkit-engine --features headless
                 cargo check -p hiwave-app --features rustkit
                 -> scripts/gate0/baseline-engine-app.txt
  renderer:      cargo check -p rustkit-renderer
                 -> scripts/gate0/baseline-renderer.txt   (its own set, counted apart)

One entry per warning, `<message> @ <file>`. The line and column are left out on purpose, so code
that moves inside a file does not read as a new warning; two warnings with the same message in the
same file are one entry. Summary lines (`N warnings generated`, `warning: `crate` (lib) generated`)
are not warnings.

usage:
  warnings.py list <log>...                       print the set, sorted
  warnings.py check <baseline> <log>...           exit 1 and name what is new (and say what is gone)
"""
import re
import sys


def census(texts):
    """The warning set of the given cargo outputs."""
    out = set()
    for text in texts:
        lines = re.sub(r"\x1b\[[0-9;]*m", "", text).split("\n")
        for i, line in enumerate(lines):
            m = re.match(r"^warning: (.*)$", line)
            if not m or re.search(r"generated \d+ warnings?|^warning: `", line) \
                    or "warning emitted" in line or "warnings emitted" in line:
                continue
            where = ""
            for k in range(i + 1, min(i + 4, len(lines))):
                mm = re.match(r"^\s*--> ([^:]+):\d+", lines[k])
                if mm:
                    where = mm.group(1)
                    break
            out.add("%s @ %s" % (m.group(1).strip(), where))
    return out


def read_baseline(path):
    with open(path, encoding="utf-8") as f:
        return {line.rstrip("\n") for line in f if line.strip() and not line.startswith("#")}


def main(argv):
    if len(argv) >= 2 and argv[0] == "list":
        found = census(open(p, errors="replace").read() for p in argv[1:])
        print("\n".join(sorted(found)))
        return 0
    if len(argv) >= 3 and argv[0] == "check":
        base = read_baseline(argv[1])
        found = census(open(p, errors="replace").read() for p in argv[2:])
        new, gone = sorted(found - base), sorted(base - found)
        print("warnings %d (baseline %d): %d new, %d gone" % (len(found), len(base), len(new), len(gone)))
        for entry in new:
            print("NEW  " + entry)
        for entry in gone:
            print("gone " + entry)
        return 1 if new else 0
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
