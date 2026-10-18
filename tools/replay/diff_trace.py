#!/usr/bin/env python3
"""Print the first divergence between a Chrome trace and a HiWave trace.

Pass the files with ``--chrome`` and ``--hiwave``. The report names both
sides with those labels.

Both files use the schema written by chrome_trace.mjs and by HiWave when
HIWAVE_TRACE_JSON or HIWAVE_CONSOLE_LOG=1 is set. Timestamps are shown and
not compared. A `load` event that only one side recorded is skipped when
both sides executed that script: HiWave does not fire `load` for a classic
external script, and that alone should not hide a later mismatch. An
`error` is never skipped.

max_js_stack is the last check. It diverges only when one side is more than
double the other and the two differ by more than 16 frames. The two engines
do not count frames the same way.

Exit status is 1 when a divergence is printed, 0 when the traces agree.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

from normalize import normalize_url

# A missing `load` is not a hard break when both sides executed the script.
SKIPPABLE = {"load"}


def _url(value: str) -> str:
    if value.startswith("inline#") or "://" not in value:
        return value
    return normalize_url(value)


def _script_key(event: dict) -> tuple:
    return (_url(str(event.get("url") or "")), str(event.get("event") or ""))


def _states(trace: dict) -> list[str]:
    return [str(item.get("state") or "") for item in trace.get("ready_state") or []]


def _console(trace: dict) -> list[str]:
    lines = []
    for item in trace.get("console") or []:
        if str(item.get("level") or "error") != "error":
            continue
        text = " ".join(str(item.get("text") or "").split())
        if text:
            lines.append(text)
    return lines


def _mutations(trace: dict) -> list[tuple]:
    found = []
    for item in trace.get("mutations") or []:
        found.append((
            str(item.get("type") or ""),
            str(item.get("target") or "").upper(),
            int(item.get("nodes") or 0),
        ))
    return found


def _align_scripts(left: list[dict], right: list[dict]) -> tuple[int, int] | None:
    """Return the first hard mismatch as indexes, or None when they agree.

    Cursors walk both lists. A lone `load` is skipped when the other side
    executed the same URL.
    """
    i = 0
    j = 0
    executed_left = { _url(str(ev.get("url") or "")) for ev in left if ev.get("event") == "execute" }
    executed_right = { _url(str(ev.get("url") or "")) for ev in right if ev.get("event") == "execute" }
    while i < len(left) or j < len(right):
        if i < len(left) and j < len(right) and _script_key(left[i]) == _script_key(right[j]):
            i += 1
            j += 1
            continue
        if i < len(left) and str(left[i].get("event")) in SKIPPABLE and _url(str(left[i].get("url") or "")) in executed_right:
            i += 1
            continue
        if j < len(right) and str(right[j].get("event")) in SKIPPABLE and _url(str(right[j].get("url") or "")) in executed_left:
            j += 1
            continue
        return (i, j)
    return None


def _format_script(trace: dict, index: int) -> str:
    scripts = trace.get("scripts") or []
    if index >= len(scripts):
        return "(none)"
    event = scripts[index]
    return f"{event.get('event')} {event.get('url')} t_ms={event.get('t_ms')}"


def _stack_diverges(left: dict, right: dict) -> bool:
    a = int(left.get("max_js_stack") or 0)
    b = int(right.get("max_js_stack") or 0)
    if abs(a - b) <= 16:
        return False
    small, large = sorted((a, b))
    if small == 0:
        return large > 16
    return large > small * 2


def first_divergence(left: dict, right: dict, left_name: str = "hiwave", right_name: str = "chrome") -> str | None:
    scripts = _align_scripts(list(left.get("scripts") or []), list(right.get("scripts") or []))
    if scripts is not None:
        i, j = scripts
        return (
            f"first divergence: scripts[{min(i, j)}]\n"
            f"  {left_name}: {_format_script(left, i)}\n"
            f"  {right_name}: {_format_script(right, j)}"
        )
    states_l, states_r = _states(left), _states(right)
    if states_l != states_r:
        return (
            f"first divergence: ready_state\n"
            f"  {left_name}: {states_l}\n"
            f"  {right_name}: {states_r}"
        )
    for name, key in (("dcl_ms", "dcl_ms"), ("load_ms", "load_ms")):
        present_l = left.get(key) is not None
        present_r = right.get(key) is not None
        if present_l != present_r:
            return (
                f"first divergence: {name}\n"
                f"  {left_name}: {left.get(key)}\n"
                f"  {right_name}: {right.get(key)}"
            )
    console_l, console_r = _console(left), _console(right)
    if console_l != console_r:
        width = max(len(console_l), len(console_r))
        for index in range(width):
            item_l = console_l[index] if index < len(console_l) else "(none)"
            item_r = console_r[index] if index < len(console_r) else "(none)"
            if item_l != item_r:
                return (
                    f"first divergence: console[{index}]\n"
                    f"  {left_name}: {item_l}\n"
                    f"  {right_name}: {item_r}"
                )
    mutations_l, mutations_r = _mutations(left), _mutations(right)
    if mutations_l != mutations_r:
        width = max(len(mutations_l), len(mutations_r))
        for index in range(width):
            item_l = mutations_l[index] if index < len(mutations_l) else "(none)"
            item_r = mutations_r[index] if index < len(mutations_r) else "(none)"
            if item_l != item_r:
                return (
                    f"first divergence: mutations[{index}]\n"
                    f"  {left_name}: {item_l}\n"
                    f"  {right_name}: {item_r}"
                )
    if _stack_diverges(left, right):
        return (
            f"first divergence: max_js_stack\n"
            f"  {left_name}: {left.get('max_js_stack')}\n"
            f"  {right_name}: {right.get('max_js_stack')}"
        )
    return None


def main() -> None:
    parser = argparse.ArgumentParser(description="Print the first divergence of two page traces")
    parser.add_argument("--chrome", required=True, help="Chrome trace JSON")
    parser.add_argument("--hiwave", required=True, help="HiWave trace JSON")
    args = parser.parse_args()
    chrome = json.loads(Path(args.chrome).read_text(encoding="utf-8"))
    hiwave = json.loads(Path(args.hiwave).read_text(encoding="utf-8"))
    found = first_divergence(chrome, hiwave, "chrome", "hiwave")
    if found is None:
        print("no divergence")
        return
    print(found)
    sys.exit(1)


if __name__ == "__main__":
    main()
