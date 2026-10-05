#!/usr/bin/env python3
"""Checks for the A/B summary and share_check's arms. Run: python3 test_ab_summary.py

1. A change that makes a load take a third layout build must not read as
   neutral. The log below has 10 pairs; in 6 of them B took a third build and
   was 40% slower, and in the other 4 the arms are equal. Dropping the
   unequal pairs (the tools' behaviour until 2026-10-05) reports 1.000 from
   4 pairs, all the information about the change removed.
2. share_check.py must set its flag in every arm. Its "off" arm used to be
   "flag unset", which stopped being off when the default flipped (#441).
"""
import os
import re
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
failures = []


def check(name, ok, detail=""):
    print(("PASS " if ok else "FAIL ") + name + ((": " + detail) if detail and not ok else ""))
    if not ok:
        failures.append(name)


lines = []
for i in range(10):
    order = "AB" if i % 4 in (0, 3) else "BA"
    slow = i < 6
    arms = {"A": "wikipedia=200/2b", "B": "wikipedia=280/3b" if slow else "wikipedia=200/2b"}
    for arm in order:
        lines.append("== pair %d order %s %s pc-x load=2.0 cnn=500/2b github=800/3b %s"
                     % (i + 1, order, arm, arms[arm]))
with tempfile.NamedTemporaryFile("w", suffix=".txt", delete=False) as f:
    f.write("\n".join(lines) + "\n")
out = subprocess.run([sys.executable, os.path.join(HERE, "ab2_summary.py"), f.name],
                     capture_output=True, text=True)
os.unlink(f.name)
text = out.stdout + out.stderr
wiki = [l for l in text.splitlines() if l.startswith("wikipedia")]
check("summary runs", out.returncode == 0 and len(wiki) == 1, text[-400:])
m = re.search(r"wikipedia all pairs: 10 pairs \(5 AB \+ 5 BA\) \| median B/A ([\d.]+)", text)
check("all 10 pairs are in the headline, median 1.400", bool(m) and m.group(1) == "1.400", text)
check("the equal-build subset is printed with its own split",
      "equal build counts only: 4 pairs (2 AB + 2 BA) | median B/A 1.000" in text, text)
check("unequal pairs are counted by arm",
      "build counts differ in 6 of 10 pairs: A built more in 0, B built more in 6" in text, text)
check("a one-sided difference is called out", "ONE-SIDED" in text, text)
check("a site with equal builds everywhere prints one line",
      "cnn all pairs: 10 pairs (5 AB + 5 BA) | median B/A 1.000" in text
      and text.count("equal build counts only") == 1, text)

src = open(os.path.join(HERE, "share_check.py")).read()
check("share_check sets the flag in every arm",
      'ARMS = (("off", "0"), ("on", "1"), ("verify", "verify"))' in src
      and "env[flag] = value" in src and "env.pop(" not in src)

sys.exit(1 if failures else 0)
