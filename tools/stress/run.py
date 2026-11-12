#!/usr/bin/env python3
"""Stress category: one run of a pinned benchmark suite, served from 127.0.0.1.

    python3 tools/stress/run.py --suite speedometer --engine hiwave --binary target/release/hiwave
    python3 tools/stress/run.py --suite motionmark  --engine hiwave-headless --binary target/release/parity-capture
    python3 tools/stress/run.py --suite jetstream   --engine chrome --binary "<Chrome for Testing>"

Suites: speedometer (3.1), motionmark (1.3.2), jetstream (2.2), pinned in
pins.json and checked out by fetch.sh under $STRESS_SUITES (default
~/zg-measure/stress-suites). Nothing is fetched at run time; the checkout
must be clean and on its pin or the run is refused.

Engines:
  hiwave           the built app in its real window, launched as the
                   real-window driver launches it (tools/real_window/driver.py:
                   a throwaway profile under $HOME, the restored tab pointed
                   at the runner URL). Takes ~/.claude/realwindow.lock, needs
                   HID idle >= 180 s and refuses 17:00-19:30 ET.
  hiwave-headless  parity-capture --url --live-ms: the same engine and live
                   loop with no window (offscreen GPU, nothing presented).
  chrome           Chrome for Testing, headless unless --headed (headed takes
                   the same lock). Its non-loopback requests go to a local
                   sink that refuses and records them.

The page reports to this server (reporter.js, added to the served runner
page; the suite on disk is not edited). Output: one JSON file per run in
--out (default ~/zg-measure/stress-runs), read by report.py.

Exit code: 0 a score was read, 1 the suite did not finish (failed, hung,
crashed, timed out), 2 the run could not start.
"""
import argparse
import datetime
import hashlib
import json
import os
import pathlib
import re
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlsplit, parse_qs

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parent.parent
sys.path.insert(0, str(REPO / "tools" / "real_window"))

PINS = json.loads((HERE / "pins.json").read_text())["suites"]
SUITES_DIR = pathlib.Path(os.environ.get("STRESS_SUITES", pathlib.Path.home() / "zg-measure" / "stress-suites"))
LOCK = pathlib.Path.home() / ".claude" / "realwindow.lock"
MIN_IDLE_S = 180
# The Mac's load rule (2026-11-11): nothing starts at a 1-minute load of
# START_LOAD or more, and a score counts only when the run started below
# SCORE_LOAD. report.py discards the rest.
START_LOAD = 6.0
SCORE_LOAD = 3.0
QUIET = (17 * 60, 19 * 60 + 30)  # no visible window, America/New_York
MOTIONMARK_TESTS = ["Multiply", "Canvas Arcs", "Leaves", "Paths", "Canvas Lines", "Images", "Design", "Suits"]

CHROME_OWN_HOSTS = {"accounts.google.com", "clients2.google.com", "content-autofill.googleapis.com",
                    "update.googleapis.com", "optimizationguide-pa.googleapis.com", "android.clients.google.com",
                    "edgedl.me.gvt1.com", "safebrowsingohttpgateway.googleapis.com"}

# Runner page (the one reporter.js is added to) and default time limits.
SUITE = {
    "speedometer": {"page": "/index.html", "timeout": 1500, "stall": 240},
    "motionmark": {"page": "/MotionMark/index.html", "timeout": 900, "stall": 180},
    "jetstream": {"page": "/index.html", "timeout": 3600, "stall": 600},
}


def runner_url(base, suite, args):
    if suite == "speedometer":
        q = "?startAutomatically=true"
        if args.iterations:
            q += "&iterationCount=%d" % args.iterations
        if args.only:
            q += "&suites=" + ",".join(args.only)
        return base + "/index.html" + q
    if suite == "motionmark":
        if args.only:
            # developer.html's documented query mode: one subtest, the
            # release runner's parameters (benchmarkDefaultParameters).
            if len(args.only) != 1:
                sys.exit("motionmark --only takes one subtest: " + ", ".join(MOTIONMARK_TESTS))
            q = ("?suite-name=MotionMark&test-name=%s&test-interval=%d&display=minimal&tiles=big&controller=ramp"
                 "&kalman-process-error=1&kalman-measurement-error=4&time-measurement=performance"
                 "&warmup-length=2000&warmup-frame-count=30&first-frame-minimum-length=0"
                 "&system-frame-rate=60&frame-rate=60"
                 % (re.sub(r"[^A-Za-z0-9]", "", args.only[0]), args.test_interval or 30))
            return base + "/MotionMark/developer.html" + q
        return base + "/MotionMark/index.html"
    q = "?report=true"
    for t in args.only or []:
        q += "&test=" + t
    return base + "/index.html" + q


# ── the loopback server ────────────────────────────────────────────────────

class Record:
    """What the page told us, with arrival times."""

    def __init__(self):
        self.lock = threading.Lock()
        self.t0 = time.time()
        self.requests = 0
        self.last = time.time()
        self.last_path = None
        self.progress = []
        self.errors = []
        self.result = None
        self.done_score = None
        self.jetstream_report = None
        self.not_found = []

    def touch(self, path):
        with self.lock:
            self.requests += 1
            self.last = time.time()
            self.last_path = path

    def at(self):
        return round(time.time() - self.t0, 2)


def make_handler(root, record, inject_page, inject_html):
    class Handler(SimpleHTTPRequestHandler):
        extensions_map = dict(SimpleHTTPRequestHandler.extensions_map, **{
            ".mjs": "text/javascript", ".js": "text/javascript", ".wasm": "application/wasm",
            ".json": "application/json", ".svg": "image/svg+xml"})

        def __init__(self, *a, **kw):
            super().__init__(*a, directory=str(root), **kw)

        def log_message(self, *a):
            pass

        def end_headers(self):
            self.send_header("Cache-Control", "no-store")
            super().end_headers()

        def reply(self, code, body=b"ok", ctype="text/plain"):
            self.send_response(code)
            self.send_header("Content-Type", ctype)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def do_GET(self):
            url = urlsplit(self.path)
            record.touch(url.path)
            if url.path == "/__stress/reporter.js":
                return self.reply(200, (HERE / "reporter.js").read_bytes(), "text/javascript")
            if url.path == "/__stress/done":
                record.done_score = parse_qs(url.query).get("score", [None])[0]
                return self.reply(200)
            if url.path in inject_page:
                f = root / url.path.lstrip("/")
                html = f.read_text(encoding="utf-8")
                head = re.search(r"<head[^>]*>", html, re.I)
                if not head:
                    return self.reply(500, b"runner page has no <head>")
                html = html[:head.end()] + inject_html + html[head.end():]
                return self.reply(200, html.encode("utf-8"), "text/html; charset=utf-8")
            if not (root / url.path.lstrip("/")).exists():
                with record.lock:
                    if len(record.not_found) < 50:
                        record.not_found.append(url.path)
            return super().do_GET()

        def do_POST(self):
            url = urlsplit(self.path)
            record.touch(url.path)
            body = self.rfile.read(int(self.headers.get("Content-Length") or 0))
            try:
                data = json.loads(body.decode("utf-8", "replace") or "null")
            except ValueError:
                data = {"unparsed": body[:300].decode("utf-8", "replace")}
            with record.lock:
                if url.path == "/__stress/progress":
                    data["t"] = record.at()
                    record.progress.append(data)
                elif url.path == "/__stress/error":
                    data["t"] = record.at()
                    record.errors.append(data)
                elif url.path == "/__stress/result":
                    record.result = data
                elif url.path == "/report":
                    record.jetstream_report = data
                else:
                    return self.reply(404, b"no such endpoint")
            self.reply(200)

    return Handler


class Sink:
    """A proxy that refuses everything and records who asked. Chrome sends
    its non-loopback requests here (loopback bypasses a proxy), so an empty
    list is a run that asked for nothing off this machine."""

    def __init__(self):
        self.hosts = []
        self.sock = socket.socket()
        self.sock.bind(("127.0.0.1", 0))
        self.sock.listen(16)
        self.port = self.sock.getsockname()[1]
        threading.Thread(target=self.loop, daemon=True).start()

    def loop(self):
        while True:
            try:
                conn, _ = self.sock.accept()
            except OSError:
                return
            try:
                conn.settimeout(2)
                line = conn.recv(2048).split(b"\r\n")[0].decode("latin-1").split(" ")
                if len(line) >= 2:
                    target = line[1]
                    host = urlsplit(target).hostname if "://" in target else target.split(":")[0]
                    self.hosts.append("%s %s" % (line[0], host))
                conn.sendall(b"HTTP/1.1 502 refused by the stress harness\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            except OSError:
                pass
            finally:
                conn.close()


# ── guards ─────────────────────────────────────────────────────────────────

def wait_for_load(limit, wait_s):
    """Block until the 1-minute load is below `limit`; False if it never was."""
    end = time.time() + wait_s
    while os.getloadavg()[0] >= limit:
        if time.time() >= end:
            return False
        time.sleep(15)
    return True


def eastern_minutes(offset_s=0):
    from zoneinfo import ZoneInfo
    now = datetime.datetime.now(ZoneInfo("America/New_York")) + datetime.timedelta(seconds=offset_s)
    return now.hour * 60 + now.minute


def hid_idle_seconds():
    r = subprocess.run(["ioreg", "-c", "IOHIDSystem"], capture_output=True, text=True)
    m = re.search(r'"HIDIdleTime" = (\d+)', r.stdout)
    return int(m.group(1)) / 1e9 if m else 0.0


class WindowLock:
    """The real-window lock, for anything that puts a window on the screen."""

    def __init__(self, limit_s, wait_s):
        self.limit_s, self.wait_s, self.held = limit_s, wait_s, False

    def __enter__(self):
        if os.environ.get("STRESS_LOCK_HELD") == "1":
            # The caller (a batch loop) holds the lock around several runs.
            if not LOCK.is_dir():
                sys.exit("STRESS_LOCK_HELD=1 but %s does not exist" % LOCK)
        else:
            end = time.time() + self.wait_s
            while True:
                try:
                    LOCK.mkdir()
                    self.held = True
                    break
                except FileExistsError:
                    if time.time() >= end:
                        print("real window busy (%s); not run" % LOCK, file=sys.stderr)
                        sys.exit(2)
                    time.sleep(10)
        try:
            # Neither the start nor the latest possible end may fall in the
            # quiet window, and the run may not span it.
            start, end = eastern_minutes(), eastern_minutes() + self.limit_s / 60.0
            if start < QUIET[1] and end >= QUIET[0]:
                raise SystemExit("a window could be up between 17:00 and 19:30 ET; not run")
            end = time.time() + self.wait_s
            while hid_idle_seconds() < MIN_IDLE_S:
                if time.time() >= end:
                    raise SystemExit("someone is using this Mac (HID idle %.0f s < %d s); not run"
                                     % (hid_idle_seconds(), MIN_IDLE_S))
                time.sleep(15)
        except SystemExit as e:
            self.__exit__()
            print(e, file=sys.stderr)
            sys.exit(2)
        return self

    def __exit__(self, *exc):
        if self.held:
            self.held = False
            try:
                LOCK.rmdir()
            except OSError:
                pass


class NoLock:
    def __enter__(self):
        return self

    def __exit__(self, *exc):
        pass


# ── engines ────────────────────────────────────────────────────────────────

class Proc:
    """A process this run started; stop() ends that pid and nothing else."""

    def __init__(self, argv, log_path, env=None, cwd=None):
        self.log_path = log_path
        self.log = open(log_path, "wb")
        self.proc = subprocess.Popen(argv, env=env, cwd=cwd, stdout=self.log, stderr=subprocess.STDOUT)

    def alive(self):
        return self.proc.poll() is None

    def stop(self):
        if self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait(timeout=5)
        self.log.close()
        return self.proc.returncode


def start_hiwave(binary, url, work, extra_env):
    """As tools/real_window/driver.py does: let the app write its own
    first-run state under a throwaway $HOME, point the restored tab at
    `url`, start again."""
    import driver as rw
    profile = work / "profile"
    profile.mkdir(parents=True)
    env = {"HIWAVE_TEST_HOOKS": "0"}
    env.update(extra_env)
    seed = rw.App(binary, profile, work / "seed.log", env).start()
    end = time.time() + 20
    while time.time() < end and seed.alive() and not seed.state_file.is_file():
        time.sleep(0.1)
    time.sleep(0.5)
    seed.stop()
    if not seed.state_file.is_file():
        raise RuntimeError("the app wrote no workspace_state.json in 20 s; see %s" % (work / "seed.log"))
    state = json.loads(seed.state_file.read_text())
    if rw.set_tab_urls(state, url) == 0:
        raise RuntimeError("no tab url in the app's workspace_state.json")
    app = rw.App(binary, profile, work / "app.log", env)
    app.state_file.write_text(json.dumps(state))
    app.start()

    class Handle:
        log_path = work / "app.log"
        proc = app.proc

        @staticmethod
        def alive():
            return app.alive()

        @staticmethod
        def stop():
            p = app.proc
            app.stop()
            return p.returncode if p else None
    return Handle


def start_headless(binary, url, work, limit_s, extra_env):
    env = dict(os.environ, **extra_env)
    argv = [str(binary), "--url", url, "--width", "1280", "--height", "800",
            "--live-ms", str(int(limit_s * 1000)), "--timeout-ms", str(int((limit_s + 120) * 1000)),
            "--post-load-live", "--animations", "--script-budget-ms", "15000", "--interrupt-scripts",
            "--event-log", str(work / "events.jsonl"), "--event-summary",
            "--event-log-error-text", "--event-log-console-text"]
    return Proc(argv, work / "app.log", env=env, cwd=str(work))


def start_chrome(binary, url, work, headed, sink):
    argv = [str(binary), "--user-data-dir=" + str(work / "chrome-profile"),
            "--no-first-run", "--no-default-browser-check", "--disable-background-networking",
            "--disable-component-update", "--disable-sync", "--disable-default-apps", "--disable-extensions",
            "--metrics-recording-only", "--password-store=basic", "--use-mock-keychain",
            "--disable-features=Translate,OptimizationHints,MediaRouter,AutofillServerCommunication",
            "--disable-client-side-phishing-detection", "--disable-domain-reliability", "--no-pings",
            "--disable-breakpad", "--no-service-autorun",
            "--window-size=1280,800", "--proxy-server=http://127.0.0.1:%d" % sink.port]
    if not headed:
        argv.append("--headless=new")
    argv.append(url)
    return Proc(argv, work / "app.log")


# ── one run ────────────────────────────────────────────────────────────────

def sha1(path):
    h = hashlib.sha1()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def check_pin(suite):
    pin = PINS[suite]
    d = SUITES_DIR / pin["dir"]
    if not (d / ".git").is_dir():
        sys.exit("no checkout at %s; run tools/stress/fetch.sh" % d)
    head = subprocess.run(["git", "-C", str(d), "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()
    dirty = subprocess.run(["git", "-C", str(d), "status", "--porcelain"], capture_output=True, text=True).stdout.strip()
    if head != pin["sha"] or dirty:
        sys.exit("%s is not on its pin (head %s, pin %s%s); run tools/stress/fetch.sh"
                 % (d, head[:12], pin["sha"][:12], ", dirty" if dirty else ""))
    return d, pin


def scores(suite, record):
    """(score, sub_scores, detail) from what the page reported."""
    if suite == "jetstream" and record.jetstream_report:
        tests = next(iter(record.jetstream_report.values()))["tests"]
        sub = {name: t["metrics"]["Score"]["current"][0] for name, t in tests.items()}
        vals = [v for v in sub.values() if isinstance(v, (int, float)) and v > 0]
        geo = None
        if vals and len(vals) == len(sub):
            import math
            geo = math.exp(sum(math.log(v) for v in vals) / len(vals))
        return geo, sub, None
    r = record.result
    if not r:
        return None, {}, None
    if suite == "speedometer":
        metrics = r.get("metrics") or {}
        # Per-workload totals in ms (lower is better); the score is upstream's.
        sub = {k: v.get("mean") for k, v in metrics.items()
               if "/" not in k and "-" in k and v.get("unit") == "ms" and not k.startswith("Iteration-")}
        detail = {k: metrics[k] for k in ("Score", "Geomean", "Total") if k in metrics}
        return r.get("score"), sub, detail
    if suite == "motionmark":
        sub = {k: v.get("score") for k, v in (r.get("tests") or {}).items()}
        return r.get("score"), sub, {"lower": r.get("lower"), "upper": r.get("upper"), "fps": r.get("fps"),
                                     "tests": r.get("tests")}
    return r.get("score"), r.get("tests") or {}, {"failedTest": r.get("failedTest")}


def log_findings(path):
    """Lines of the app's log that say why a page stopped (shapes only)."""
    try:
        text = re.sub(r"\x1b\[[0-9;]*m", "", pathlib.Path(path).read_text(errors="replace"))
    except OSError:
        return [], ""
    pat = re.compile(r"Script stopped|panicked|stack overflow|RuntimeLimit|uncatchable|SIGSEGV|SIGABRT|fatal runtime", re.I)
    hits = [line.strip()[:300] for line in text.splitlines() if pat.search(line)]
    return hits[:20], text[-1500:]


def headless_events(path):
    """From parity-capture's event log: request hosts and script errors."""
    hosts, errors, unsupported = {}, [], {}
    try:
        lines = pathlib.Path(path).read_text(errors="replace").splitlines()
    except OSError:
        return None
    for line in lines:
        try:
            ev = json.loads(line)
        except ValueError:
            continue
        blob = json.dumps(ev)
        for m in re.finditer(r'"url":\s*"(https?://[^"/]+)', blob):
            hosts[m.group(1)] = hosts.get(m.group(1), 0) + 1
        kind = str(ev.get("kind") or ev.get("type") or "")
        if "error" in kind.lower() and len(errors) < 40:
            errors.append(ev)
        if "unsupported" in kind.lower():
            key = str(ev.get("member") or ev.get("name") or ev.get("api") or blob[:120])
            unsupported[key] = unsupported.get(key, 0) + 1
    return {"request_origins": hosts,
            "non_loopback_origins": [h for h in hosts if urlsplit(h).hostname not in ("127.0.0.1", "localhost", "::1")],
            "script_errors": errors, "unsupported_apis": unsupported, "events": len(lines)}


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--suite", required=True, choices=sorted(SUITE))
    ap.add_argument("--engine", required=True, choices=["hiwave", "hiwave-headless", "chrome"])
    ap.add_argument("--binary", required=True, help="hiwave, parity-capture or Chrome for Testing executable")
    ap.add_argument("--label", help="name for this binary in reports (default: its sha1's first 8)")
    ap.add_argument("--out", default=str(pathlib.Path.home() / "zg-measure" / "stress-runs"))
    ap.add_argument("--timeout", type=int, help="wall limit for the run, seconds")
    ap.add_argument("--stall", type=int, help="seconds with nothing from the page before the run is called hung")
    ap.add_argument("--only", action="append", help="one workload/subtest (repeatable); a partial run, never a suite score")
    ap.add_argument("--iterations", type=int, help="speedometer iterationCount (default: upstream's 10); partial")
    ap.add_argument("--test-interval", type=int, help="motionmark --only: seconds per subtest (default 30)")
    ap.add_argument("--trace", action="store_true", help="report every subtest start (more requests; for finding a hang)")
    ap.add_argument("--headed", action="store_true", help="chrome: a visible window (takes the real-window lock)")
    ap.add_argument("--lock-wait", type=int, default=1200, help="seconds to wait for the lock and for an idle seat")
    ap.add_argument("--max-load", type=float, default=SCORE_LOAD,
                    help="start only below this 1-minute load (default %.0f, where a score counts; at most %.0f, "
                         "for runs that only look for where a suite stops)" % (SCORE_LOAD, START_LOAD))
    ap.add_argument("--load-wait", type=int, default=1800, help="seconds to wait for the load to come down")
    ap.add_argument("--env", action="append", default=[], help="KEY=VALUE for the engine process (repeatable)")
    ap.add_argument("--until-exit", action="store_true",
                    help="hiwave-headless: never stop the engine early, so its event log (requests, script errors) is written")
    ap.add_argument("--keep", action="store_true", help="keep the work directory (profile, logs)")
    args = ap.parse_args()

    binary = pathlib.Path(args.binary).resolve()
    if not binary.is_file():
        sys.exit("no binary at %s" % binary)
    max_load = min(args.max_load, START_LOAD)
    if not wait_for_load(max_load, args.load_wait):
        print("1-minute load %.1f stayed at or above %.1f for %d s; not run"
              % (os.getloadavg()[0], max_load, args.load_wait), file=sys.stderr)
        return 2
    root, pin = check_pin(args.suite)
    limit = args.timeout or SUITE[args.suite]["timeout"]
    stall = args.stall or SUITE[args.suite]["stall"]
    extra_env = dict(kv.split("=", 1) for kv in args.env)
    out = pathlib.Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    bin_sha = sha1(binary)
    label = args.label or bin_sha[:8]
    stamp = datetime.datetime.now().strftime("%Y%m%dT%H%M%S")
    name = "%s_%s_%s_%s" % (args.suite, args.engine, label, stamp)
    work = pathlib.Path(tempfile.mkdtemp(prefix="stress-" + name + "-"))

    record = Record()
    inject = ('<script>window.__stress=%s;</script><script src="/__stress/reporter.js"></script>'
              % json.dumps({"suite": args.suite, "trace": args.trace,
                            "start": args.suite == "motionmark" and not args.only}))
    pages = {SUITE[args.suite]["page"], "/MotionMark/developer.html", "/MotionMark/", "/"} if args.suite == "motionmark" \
        else {SUITE[args.suite]["page"], "/"}
    pages = {p for p in pages if (root / p.lstrip("/")).is_file()}
    httpd = ThreadingHTTPServer(("127.0.0.1", 0), make_handler(root, record, pages, inject))
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    base = "http://127.0.0.1:%d" % httpd.server_address[1]
    url = runner_url(base, args.suite, args)

    visible = args.engine == "hiwave" or (args.engine == "chrome" and args.headed)
    sink = Sink() if args.engine == "chrome" else None
    handle = None
    status, exit_code = "not-started", None

    def bail(signum, frame):
        raise KeyboardInterrupt

    signal.signal(signal.SIGTERM, bail)
    signal.signal(signal.SIGHUP, bail)
    load_before = os.getloadavg()
    started = time.time()
    try:
        with (WindowLock(limit + 60, args.lock_wait) if visible else NoLock()):
            try:
                if not wait_for_load(max_load, args.load_wait):
                    print("load rose to %.1f while waiting for the window; not run" % os.getloadavg()[0], file=sys.stderr)
                    httpd.shutdown()
                    shutil.rmtree(work, ignore_errors=True)
                    sys.exit(2)  # the lock is released on the way out
                load_before = os.getloadavg()
                started = time.time()
                record.t0 = record.last = started
                if args.engine == "hiwave":
                    handle = start_hiwave(binary, url, work, extra_env)
                    record.t0 = record.last = started = time.time()
                elif args.engine == "hiwave-headless":
                    handle = start_headless(binary, url, work, limit, extra_env)
                else:
                    handle = start_chrome(binary, url, work, args.headed, sink)
                status = "running"
                # --until-exit (headless): parity-capture writes its event log
                # when its live window (--timeout) ends, so let it end.
                until_exit = args.until_exit and args.engine == "hiwave-headless"
                verdict = None
                while True:
                    time.sleep(0.5)
                    if not handle.alive():
                        status = verdict or "exited"
                        break
                    if verdict is None and (record.result is not None or record.jetstream_report is not None):
                        time.sleep(1.0)  # let /report or the result's twin arrive
                        failed = bool(record.result and record.result.get("failed")) and not record.jetstream_report
                        verdict = "failed" if failed else "ok"
                    if verdict is None and time.time() - started > limit and not until_exit:
                        verdict = "timeout"
                    if verdict is None and time.time() - record.last > stall:
                        verdict = "hung"
                    if until_exit and time.time() - started < limit + 150:
                        continue
                    if verdict:
                        status = verdict
                        break
            except KeyboardInterrupt:
                status = "interrupted"
            finally:
                if handle is not None:
                    exit_code = handle.stop()
    finally:
        httpd.shutdown()

    wall = round(time.time() - started, 1)
    score, sub, detail = scores(args.suite, record)
    partial = bool(args.only or args.iterations or args.test_interval)
    if status == "ok" and score is None:
        status = "failed"
    findings, tail = log_findings(work / "app.log") if handle is not None else ([], "")
    run = {
        "schema": 1,
        "suite": args.suite,
        "suite_version": pin["version"],
        "suite_sha": pin["sha"],
        "engine": args.engine,
        "headed": bool(visible),
        "binary": str(binary),
        "binary_sha1": bin_sha,
        "label": label,
        "url": url.replace(base, "http://127.0.0.1:<port>"),
        "partial": partial,
        "status": status,
        "score": score,
        "sub_scores": sub,
        "detail": detail,
        "wall_s": wall,
        "load_avg_before": [round(x, 2) for x in load_before],
        "load_valid": load_before[0] < SCORE_LOAD,
        "load_avg_after": [round(x, 2) for x in os.getloadavg()],
        "started": datetime.datetime.fromtimestamp(started).astimezone().isoformat(timespec="seconds"),
        "exit_code": exit_code,
        "errors": record.errors,
        "progress_count": len(record.progress),
        "last_progress": record.progress[-6:],
        "requests": record.requests,
        "last_request": record.last_path,
        "seconds_since_last_request": round(time.time() - record.last, 1),
        "not_found": record.not_found,
        "done_get_score": record.done_score,
        "log_findings": findings,
        "host": {"cpus": os.cpu_count(), "machine": os.uname().machine, "os": os.uname().release},
    }
    if status not in ("ok",):
        run["log_tail"] = tail
    if sink is not None:
        # Chrome asks these for itself (sign-in, component updates, autofill)
        # whatever page is open; the sink refused them too. Anything else is
        # the page's and is a finding.
        seen = sorted(set(sink.hosts))
        run["browser_own_requests"] = [h for h in seen if h.split(" ")[-1] in CHROME_OWN_HOSTS]
        run["non_loopback_requests"] = [h for h in seen if h.split(" ")[-1] not in CHROME_OWN_HOSTS]
    if args.engine == "hiwave-headless":
        run["headless_events"] = headless_events(work / "events.jsonl")
    path = out / (name + ".json")
    path.write_text(json.dumps(run, indent=1) + "\n")
    if args.keep or status != "ok":
        keep = out / (name + ".work")
        for f in ("app.log", "seed.log", "events.jsonl"):
            if (work / f).is_file():
                keep.mkdir(exist_ok=True)
                shutil.copy(work / f, keep / f)
    shutil.rmtree(work, ignore_errors=True)
    print("%s %s %s: %s%s score=%s wall=%ss errors=%d -> %s"
          % (args.suite, args.engine, label, status, "" if run["load_valid"] else " (load %.1f: score not valid)" % load_before[0], "%.2f" % score if isinstance(score, (int, float)) else score,
             wall, len(record.errors), path))
    return 0 if status == "ok" else 1


if __name__ == "__main__":
    sys.exit(main())
