"""Append <src> to <dst> once. usage: append.py <dst> <src> <marker>"""
import sys

dst, src, marker = sys.argv[1:4]
s = open(dst).read()
if marker in s:
    raise SystemExit("already there")
if not s.endswith("\n"):
    s += "\n"
open(dst, "w").write(s + open(src).read())
print("appended")
