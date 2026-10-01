"""Chrome's rects and computed font for kbd boxes in a builtins case.
usage: kbd_rects.py <case> [n]"""
import json, sys

case = sys.argv[1]
n = int(sys.argv[2]) if len(sys.argv) > 2 else 12
B = f"/Users/petecopeland/Repos/.worktrees/rs-dev-9f49a40/baselines/chrome-148/builtins/{case}"
rects = json.load(open(f"{B}/layout-rects.json"))
styles = json.load(open(f"{B}/computed-styles.json"))
print(type(rects).__name__, list(rects.keys())[:8] if isinstance(rects, dict) else rects[0])
print(type(styles).__name__, list(styles.keys())[:8] if isinstance(styles, dict) else styles[0])
items = rects.get("elements") or rects.get("rects") or rects.get("boxes") if isinstance(rects, dict) else rects
shown = 0
for it in items:
    s = json.dumps(it)
    if "kbd" in s.lower() and shown < n:
        print(s[:400])
        shown += 1
sitems = styles.get("elements") or styles.get("styles") if isinstance(styles, dict) else styles
shown = 0
for it in (sitems.values() if isinstance(sitems, dict) else sitems):
    s = json.dumps(it)
    if "kbd" in s.lower() and shown < 2:
        print(s[:1500])
        shown += 1
