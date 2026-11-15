"""What the engine does with one long turn, on long_turn_engine.html (the page long_turn_chrome.mjs runs in
Chrome): parity-capture's live loop as tools/stress runs it, once at the engine's default live-turn budget
and once raised (--live-turn-budget-ms), reading the page's own record from its console line.

    python long_turn_engine.py <parity-capture> <long_turn_engine.html> <results.json> [raised_ms]
"""
import functools
import http.server
import json
import pathlib
import subprocess
import sys
import tempfile
import threading


def rec_in(v):
    """The page's record in one event, wherever its console text sits."""
    if isinstance(v, str):
        return json.loads(v[4:]) if v.startswith("REC {") else None
    for x in (v.values() if isinstance(v, dict) else v if isinstance(v, list) else ()):
        found = rec_in(x)
        if found is not None:
            return found
    return None


def run(binary, page, budget):
    # Served from this machine's loopback only, as tools/stress serves a suite.
    page = pathlib.Path(page).resolve()
    handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=str(page.parent))
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        return capture(binary, "http://127.0.0.1:%d/%s" % (server.server_address[1], page.name), budget)
    finally:
        server.shutdown()


def capture(binary, url, budget):
    with tempfile.TemporaryDirectory() as work:
        events = pathlib.Path(work) / "events.jsonl"
        argv = [binary, "--url", url, "--width", "1280", "--height", "800",
                "--live-ms", "24000", "--timeout-ms", "120000", "--post-load-live", "--animations",
                "--script-budget-ms", "15000", "--interrupt-scripts", "--event-log", str(events),
                "--event-log-console-text"]
        if budget is not None:
            argv += ["--live-turn-budget-ms", str(budget)]
        p = subprocess.run(argv, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=180)
        lines = events.read_text(encoding="utf-8", errors="replace").splitlines() if events.is_file() else []
        for line in lines:
            try:
                found = rec_in(json.loads(line))
            except ValueError:
                continue
            if found is not None:
                return {"exit": p.returncode, **found}
        return {"exit": p.returncode, "rec": None}


def main():
    binary, page, out = sys.argv[1:4]
    raised = int(sys.argv[4]) if len(sys.argv) > 4 else 60000
    res = {"page": "long_turn_engine.html",
           "default_budget": run(binary, page, None),
           "raised_budget_%d" % raised: run(binary, page, raised)}
    pathlib.Path(out).write_text(json.dumps(res, indent=1), encoding="utf-8")
    print(json.dumps(res))


if __name__ == "__main__":
    main()
