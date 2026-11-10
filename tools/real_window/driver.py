#!/usr/bin/env python3
"""Real-window driver for the built HiWave app on macOS (Z2-I2).

Launches the app binary you name with a throwaway profile, points it at a
local fixture page, drives the real window (HID events through hwdrive,
Accessibility for resize, screencapture for frames) and asserts on three
things: what the fixture server was asked for, what the app logged, and what
colours are in the window.

    python3 tools/real_window/driver.py --app <path to hiwave> [--out DIR]
                                        [--checks h1,h4] [--list]

Each assertion ends as PASS, FAIL or NOT RUN. NOT RUN names what the seat
lacks (a grant, or an unlocked screen); it is never counted as a pass.

Exit code: 0 every assertion passed, 1 at least one failed, 3 none failed but
at least one did not run, 2 the driver itself could not run.

What needs what:
  server and log assertions   nothing
  frames                      Screen Recording grant, screen unlocked
  move/click/scroll/keys      Accessibility grant, screen unlocked
  resize                      Accessibility grant, screen unlocked
The grants belong to the program that starts this script (the terminal, or
the launchd job's program), not to the script.

The profile is isolated by setting HOME for the app only: it reads its data
directory from $HOME, so the person's own tabs and vault are not touched.
"""
import argparse
import hashlib
import http.server
import json
import os
import pathlib
import re
import shutil
import struct
import subprocess
import sys
import threading
import time
import urllib.parse
import zlib

HERE = pathlib.Path(__file__).resolve().parent
PAGES = HERE / "pages"
NEW_TAB_URL = "hiwave://newtab"
ANSI = re.compile(r"\x1b\[[0-9;]*m")
# The RustKit chrome is the app's default on macOS (CH3.A5a); a check that
# covers the WebKit chrome (the escape hatch) launches with this.
WEBKIT_CHROME = {"HIWAVE_RUSTKIT_CHROME": "0"}

RED, GREEN, BLUE = (255, 0, 0), (0, 255, 0), (0, 0, 255)
MAGENTA, CYAN = (255, 0, 255), (0, 255, 255)


# ── fixture server ──────────────────────────────────────────────────────────

class HidUnsafe(Exception):
    """The seat is not safe to drive: the app is not frontmost, or a person is using it."""


def flat_png(rgb, width=120, height=60):
    """A flat-colour PNG, built by hand so the fixture needs no image library."""
    def chunk(tag, data):
        body = tag + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))
    row = b"\x00" + bytes(rgb) * width
    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(row * height))
            + chunk(b"IEND", b""))


class Fixture:
    """Serves tools/real_window/pages and remembers every request it got."""

    def __init__(self):
        self.requests = []  # (seconds since start, path with query)
        self.reply = b"ok"  # what a beacon is answered with
        self.lock = threading.Lock()
        self.t0 = time.time()
        fixture = self

        class Handler(http.server.BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def send(self, code, ctype, body):
                self.send_response(code)
                self.send_header("Content-Type", ctype)
                self.send_header("Content-Length", str(len(body)))
                self.send_header("Cache-Control", "no-store")
                self.end_headers()
                self.wfile.write(body)

            def do_GET(self):
                with fixture.lock:
                    fixture.requests.append((round(time.time() - fixture.t0, 3), self.path))
                url = urllib.parse.urlparse(self.path)
                if url.path == "/beacon":
                    return self.send(200, "text/plain", fixture.reply)
                if url.path == "/e9_start":
                    # CH2.E9: the tab is restored here and lands on e9_final.
                    self.send_response(302)
                    self.send_header("Location", "/e9_final.html")
                    self.send_header("Content-Length", "0")
                    self.end_headers()
                    return
                if url.path == "/slow":
                    ms = int(urllib.parse.parse_qs(url.query).get("ms", ["1000"])[0])
                    time.sleep(ms / 1000)
                    return self.send(200, "text/plain", b"late")
                m = re.fullmatch(r"/(slow)?img/([0-9a-f]{6})\.png", url.path)
                if m:
                    if m.group(1):
                        time.sleep(int(urllib.parse.parse_qs(url.query).get("ms", ["1000"])[0]) / 1000)
                    return self.send(200, "image/png", flat_png(bytes.fromhex(m.group(2))))
                page = PAGES / url.path.lstrip("/")
                if page.is_file() and page.parent == PAGES:
                    # h8's page comes as a server's error page does.
                    code = 403 if page.name == "h8_error.html" else 200
                    return self.send(code, "text/html; charset=utf-8", page.read_bytes())
                self.send(404, "text/plain", b"not found")

        self.httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.base = "http://127.0.0.1:%d" % self.httpd.server_address[1]
        threading.Thread(target=self.httpd.serve_forever, daemon=True).start()

    def seen(self, needle, since=0):
        with self.lock:
            return [p for (_, p) in self.requests[since:] if needle in p]

    def rows(self, needle, since=0):
        with self.lock:
            return [(t, p) for (t, p) in self.requests[since:] if needle in p]

    def times(self, needle, since=0):
        """Seconds from the first request at or after `since` to each match."""
        with self.lock:
            rows = self.requests[since:]
        return [round(t - rows[0][0], 1) for (t, p) in rows if needle in p]

    def mark(self):
        with self.lock:
            return len(self.requests)

    def wait(self, needle, timeout, since=0, count=1):
        end = time.time() + timeout
        while time.time() < end:
            if len(self.seen(needle, since)) >= count:
                return True
            time.sleep(0.05)
        return len(self.seen(needle, since)) >= count


# ── the app ────────────────────────────────────────────────────────────────

class App:
    def __init__(self, binary, profile, log_path, env=None):
        self.binary, self.profile, self.log_path = binary, profile, log_path
        self.env = env or {}
        self.proc = None

    @property
    def state_file(self):
        return self.profile / "Library" / "Application Support" / "hiwave" / "workspace_state.json"

    def start(self):
        # HIWAVE_TEST_HOOKS=1: the app runs HIWAVE_RUSTKIT_CHROME_EVAL and
        # logs IPC bodies whole, which the checks read (#907 R1 nits 1, 2).
        # A RustKit panel's IPC is never logged whole (#910 R1).
        # A check may turn the hooks off (a6_url_log reads the default log).
        env = dict(os.environ, HOME=str(self.profile), HIWAVE_TEST_HOOKS="1")
        env.update(self.env)
        self.log = open(self.log_path, "wb")
        self.proc = subprocess.Popen([str(self.binary)], env=env, stdout=self.log,
                                     stderr=subprocess.STDOUT, cwd=str(self.profile))
        return self

    def log_text(self):
        try:
            return ANSI.sub("", self.log_path.read_text(errors="replace"))
        except FileNotFoundError:
            return ""

    def wait_log(self, pattern, timeout, start=0):
        """The first match of `pattern` at or after offset `start`, or None."""
        end = time.time() + timeout
        while True:
            m = re.search(pattern, self.log_text()[start:])
            if m or time.time() >= end or self.proc.poll() is not None:
                return m
            time.sleep(0.1)

    def alive(self):
        return self.proc is not None and self.proc.poll() is None

    def stop(self):
        if self.proc is None:
            return
        if self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=4)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait(timeout=4)
        self.log.close()
        self.proc = None


def tab_urls(node):
    """Every `url` string in the saved state."""
    found = []
    if isinstance(node, dict):
        for key, value in node.items():
            if key == "url" and isinstance(value, str):
                found.append(value)
            else:
                found.extend(tab_urls(value))
    elif isinstance(node, list):
        for value in node:
            found.extend(tab_urls(value))
    return found


def set_tab_urls(node, url):
    """Point every restored tab at `url`. Returns how many it changed."""
    changed = 0
    if isinstance(node, dict):
        for key, value in node.items():
            if key == "url" and isinstance(value, str):
                node[key] = url
                changed += 1
            else:
                changed += set_tab_urls(value, url)
    elif isinstance(node, list):
        for value in node:
            changed += set_tab_urls(value, url)
    return changed


# ── frames ─────────────────────────────────────────────────────────────────

def resize_ack(out):
    """The acknowledgement `hwdrive resize` measured itself, from its JSON
    line: (ack_ms, ax_ms). ack_ms is request to the window server showing
    the asked size; None if the window did not reach it within hwdrive's
    2 s, or if this hwdrive predates the field (it then slept a fixed
    400 ms and timed nothing)."""
    def ms(key):
        v = out.get(key) if isinstance(out, dict) else None
        return float(v) if isinstance(v, (int, float)) and not isinstance(v, bool) else None
    return ms("ack_ms"), ms("ax_ms")


def resize_request_epoch(out):
    """hwdrive's own wall-clock time of the resize request (`req_epoch`,
    Unix seconds, the clock the app log's UTC stamps use), or None."""
    v = out.get("req_epoch") if isinstance(out, dict) else None
    return float(v) if isinstance(v, (int, float)) and not isinstance(v, bool) else None


def page_view_id(log, base):
    """The engine view the fixture page was loaded into: the first
    `Loading URL id=EngineViewId(N) url=<fixture base>/...` line. The
    chrome is another view and loads no fixture URL."""
    m = re.search(r"Loading URL id=EngineViewId\((\d+)\) url=%s/" % re.escape(base), log)
    return int(m.group(1)) if m else None


RELAYOUT_RESIZE = re.compile(
    r"^(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?)Z\b.*?"
    r"relayout\{id=EngineViewId\((\d+)\).*\brelayout cause=resize\b", re.M)


def resize_relayouts(log, t0):
    """For information: each engine view's first `relayout cause=resize`
    line stamped at or after `t0` (epoch seconds), as {view id: ms after t0},
    from the app log's own UTC timestamps. That is when the view laid its
    page out at the new size, which is a separate figure from the window's
    acknowledgement: a page whose thread is busy (a load, a runaway script)
    lays out later."""
    import datetime
    got = {}
    for m in RELAYOUT_RESIZE.finditer(log):
        when = datetime.datetime.fromisoformat(m.group(1)[:26]).replace(tzinfo=datetime.timezone.utc).timestamp()
        view = int(m.group(2))
        if when >= t0 and view not in got:
            got[view] = round((when - t0) * 1000, 1)
    return got


def count_colours(png_path, colours, tolerance=48):
    """How many pixels of the frame are within `tolerance` of each colour."""
    from PIL import Image
    img = Image.open(png_path).convert("RGB")
    counts = {c: 0 for c in colours}
    for (n, rgb) in img.getcolors(maxcolors=img.width * img.height):
        for c in colours:
            if all(abs(rgb[i] - c[i]) <= tolerance for i in range(3)):
                counts[c] += n
    return counts, img.size


# ── one run ────────────────────────────────────────────────────────────────

class Driver:
    def __init__(self, args):
        self.binary = pathlib.Path(args.app).resolve()
        self.out = pathlib.Path(args.out).resolve()
        self.out.mkdir(parents=True, exist_ok=True)
        self.fixture = Fixture()
        self.hwdrive = self.build_hwdrive(args.hwdrive)
        self.pre = self.hw("preflight")[1]
        unlocked = not self.pre.get("screen_locked", True)
        self.can_input = bool(self.pre.get("post_events")) and unlocked
        self.can_resize = bool(self.pre.get("accessibility")) and unlocked
        self.can_capture = bool(self.pre.get("screen_capture")) and unlocked
        self.results = []  # (check, assertion, status, detail)
        self.app = None
        self.template = None

    def why_not(self, grant):
        reasons = []
        if self.pre.get("screen_locked", True):
            reasons.append("the screen is locked")
        key = "screen_capture" if grant == "Screen Recording" else "accessibility"
        if not self.pre.get(key):
            reasons.append("no %s grant" % grant)
        return " and ".join(reasons)

    def build_hwdrive(self, given):
        if given:
            return pathlib.Path(given).resolve()
        exe = self.out / "hwdrive"
        r = subprocess.run(["swiftc", "-O", "-o", str(exe), str(HERE / "hwdrive.swift")],
                           capture_output=True, text=True)
        if r.returncode != 0:
            sys.exit("hwdrive did not compile:\n" + r.stderr[-2000:])
        return exe

    # HID events go to whatever window is under the pointer or has the
    # keyboard, not to a pid: before each one, the app must be frontmost and
    # nobody else must be using the machine (11-05 HID-safety rule).
    HID_COMMANDS = {"move", "click", "press", "release", "dblclick", "rightclick", "rightpress", "drag",
                    "scroll", "key", "type"}
    MIN_IDLE_S = 30

    @staticmethod
    def frontmost_pid():
        r = subprocess.run(["lsappinfo", "info", "-only", "pid", subprocess.run(
            ["lsappinfo", "front"], capture_output=True, text=True).stdout.strip()], capture_output=True, text=True)
        m = re.search(r"=(\d+)", r.stdout)
        return int(m.group(1)) if m else None

    @staticmethod
    def hid_idle_seconds():
        r = subprocess.run(["ioreg", "-c", "IOHIDSystem"], capture_output=True, text=True)
        m = re.search(r'"HIDIdleTime" = (\d+)', r.stdout)
        return int(m.group(1)) / 1e9 if m else 0.0

    def hid_guard(self, argv):
        """Raise HidUnsafe unless the target pid is frontmost and the seat is idle."""
        pid = int(argv[1])
        # Our own events reset the idle clock: the seat is someone else's when
        # it went idle more recently than our last event (or than the floor).
        idle = self.hid_idle_seconds()
        since_ours = time.time() - self.last_hid()
        if idle + 1.0 < min(self.MIN_IDLE_S, since_ours):
            raise HidUnsafe("someone is using this Mac (HID idle %.1f s, our last event %.1f s ago)"
                            % (idle, since_ours))
        if self.frontmost_pid() != pid:
            subprocess.run([str(self.hwdrive), "activate", str(pid)], capture_output=True, text=True)
            if self.frontmost_pid() != pid:
                raise HidUnsafe("pid %d is not frontmost (frontmost %s)" % (pid, self.frontmost_pid()))

    # Our last event's time is kept beside the real-window lock (outside it,
    # so the lock stays an empty directory), so the next
    # run's first event does not read our own as a person's.
    LAST_HID = pathlib.Path.home() / ".claude" / "realwindow.last_hid"

    def last_hid(self):
        try:
            return float(self.LAST_HID.read_text())
        except (OSError, ValueError):
            return getattr(self, "_last_hid", 0.0)

    def note_hid(self):
        self._last_hid = time.time()
        try:
            self.LAST_HID.write_text(str(self._last_hid))
        except OSError:
            pass

    def hw(self, *argv):
        if argv and argv[0] in self.HID_COMMANDS:
            self.hid_guard(argv)
        r = subprocess.run([str(self.hwdrive)] + [str(a) for a in argv], capture_output=True, text=True)
        if argv and argv[0] in self.HID_COMMANDS:
            self.note_hid()
            if r.returncode == 5:
                raise HidUnsafe(r.stdout.strip()[-300:])
        try:
            return r.returncode, json.loads(r.stdout.strip().splitlines()[-1])
        except (ValueError, IndexError):
            return r.returncode, {"error": (r.stdout + r.stderr).strip()[-300:]}

    # assertions
    def note(self, check, name, status, detail=""):
        self.results.append((check, name, status, detail))
        print("  %-8s %s%s" % (status, name, (" — " + detail) if detail else ""), flush=True)

    def expect(self, check, name, ok, detail=""):
        self.note(check, name, "PASS" if ok else "FAIL", detail)
        return ok

    def not_run(self, check, name, reason):
        self.note(check, name, "NOT RUN", reason)

    # app lifecycle
    def seed_profile(self, profile):
        """Let the app write its own first-run state, so the driver does not
        carry a copy of the file's format."""
        app = App(self.binary, profile, self.out / "seed.log").start()
        end = time.time() + 20
        while time.time() < end and app.alive() and not app.state_file.is_file():
            time.sleep(0.1)
        time.sleep(0.5)
        app.stop()
        if not app.state_file.is_file():
            sys.exit("the app wrote no workspace_state.json in 20 s; see %s" % (self.out / "seed.log"))
        self.template = app.state_file.read_text()
        if set_tab_urls(json.loads(self.template), "x") == 0:
            sys.exit("no tab url found in the app's workspace_state.json")

    def open_page(self, check, page, env=None):
        """A fresh app on `page`. Returns once the engine has started the load
        (or failed to), with the outcome recorded."""
        profile, seed = self.out / "profile", self.out / "profile-seed"
        if self.template is None:
            shutil.rmtree(profile, ignore_errors=True)
            profile.mkdir(parents=True, exist_ok=True)
            self.seed_profile(profile)
            shutil.rmtree(seed, ignore_errors=True)
            shutil.copytree(profile, seed, symlinks=True)
        # Every check starts on the profile as the seed launch left it, not on
        # what an earlier check saved: a4_settings saves 'sidebar open by
        # default' unticked, and a later check on that profile launched with
        # no sidebar and clicked where its button or a tab would have been
        # (CH3.A4b).
        shutil.rmtree(profile, ignore_errors=True)
        shutil.copytree(seed, profile, symlinks=True)
        url = "%s/%s" % (self.fixture.base, page)
        state = json.loads(self.template)
        set_tab_urls(state, url)
        app = App(self.binary, profile, self.out / (check + ".app.log"), env)
        app.state_file.write_text(json.dumps(state))
        self.since = self.fixture.mark()
        self.app = app.start()
        got = self.fixture.wait("/" + page.split("?")[0], 30, self.since)
        self.expect(check, "the app fetched the page it was restored on", got, url)
        if got:
            # The engine logs this line when a document's first layout is built.
            self.app.wait_log(r"Root box built", 20)
            time.sleep(0.5)
        self.window = None
        code, win = self.hw("window", self.app.proc.pid) if self.app.alive() else (2, {})
        if code == 0:
            self.window = win
        self.expect(check, "the app has a window", code == 0, json.dumps(win))
        if self.can_input:
            self.hw("activate", self.app.proc.pid)
        return got and code == 0

    def close_page(self):
        if self.app:
            self.app.stop()
            self.app = None

    # window
    def centre(self):
        code, win = self.hw("window", self.app.proc.pid)
        return win["w"] / 2, win["h"] / 2

    def frame(self, check, name, colours):
        """Capture the window and count the given colours. None if it cannot."""
        if not self.can_capture:
            return None
        path = self.out / ("%s.%s.png" % (check, name))
        code, win = self.hw("window", self.app.proc.pid)
        r = subprocess.run(["/usr/sbin/screencapture", "-x", "-o", "-l", str(win.get("id", 0)), str(path)],
                           capture_output=True, text=True)
        if r.returncode != 0 or not path.is_file():
            self.note(check, "capture " + name, "NOT RUN", "screencapture failed: " + r.stderr.strip()[:120])
            return None
        counts, size = count_colours(path, colours)
        counts["size"] = size
        return counts

    # ── checks ──────────────────────────────────────────────────────────────

    def h1(self):
        """H1: a page keeps changing after its load (ticks, a late fetch, a late image)."""
        c = "h1"
        if not self.open_page(c, "h1_late.html"):
            return
        self.expect(c, "the page's script ran", self.fixture.wait("beacon?h1-script", 10, self.since))
        first = self.frame(c, "first", [RED, GREEN, BLUE, CYAN])
        done = self.app.wait_log(r"Navigation finished.*", 20)
        self.expect(c, "the load finished", bool(done), done.group(0)[:80] if done else "")
        log_from = done.end() if done else 0
        ticks = self.fixture.wait("beacon?h1-tick-8", 20, self.since)
        self.expect(c, "eight one-second ticks ran", ticks,
                    "seconds after the page request: %s" % self.fixture.times("beacon?h1-tick-", self.since))
        self.expect(c, "a fetch started after the load reached its then()",
                    self.fixture.wait("beacon?h1-fetch", 6, self.since),
                    "at %s s" % self.fixture.times("beacon?h1-fetch", self.since))
        self.expect(c, "the app asked for an image a tick added",
                    self.fixture.wait("/img/00ffff.png", 6, self.since),
                    "at %s s" % self.fixture.times("/img/00ffff.png", self.since))
        m = self.app.wait_log(r"Loaded images added by live page scripts", 3, log_from)
        self.expect(c, "the engine logged the late image as loaded by the live loop", bool(m))
        time.sleep(1.5)
        late = self.frame(c, "late", [RED, GREEN, BLUE, CYAN])
        if first is None or late is None:
            return self.not_run(c, "the window shows the late bands and image", self.why_not("Screen Recording"))
        self.expect(c, "the first frame shows the red band", first[RED] > 1000, str(first))
        self.expect(c, "the window shows more green bands than the first frame",
                    late[GREEN] > first[GREEN] and late[GREEN] > 1000, "first %s, late %s" % (first, late))
        self.expect(c, "the window shows the late fetch's blue band", late[BLUE] > 1000, str(late))
        self.expect(c, "the window shows the late image", late[CYAN] > 2000, str(late))

    def widest_gap(self, tick, start, end):
        """The longest stretch of [start, end] with no `tick` beacon, and how many there were."""
        ticks = [t for (t, _) in self.fixture.rows(tick, self.since) if start <= t <= end]
        edges = [start] + ticks + [end]
        return round(max(b - a for a, b in zip(edges, edges[1:])), 2), len(ticks)

    def h4_slow(self):
        """H4: an image that takes three seconds does not stop the page's clock, and arrives."""
        c = "h4_slow"
        if not self.open_page(c, "h4_slow.html"):
            return
        self.app.wait_log(r"Navigation finished", 20)
        time.sleep(2.5)
        now = time.time() - self.fixture.t0
        gap, count = self.widest_gap("beacon?img-tick-", now - 2.0, now)
        self.expect(c, "with nothing pending, the 200 ms ticks arrive steadily", gap <= 0.6,
                    "%d ticks in 2 s, widest gap %.2f s" % (count, gap))
        mark, log_from = self.fixture.mark(), len(self.app.log_text())
        self.fixture.reply = b"go"
        started = self.fixture.wait("/slowimg/", 4, mark)
        self.fixture.reply = b"ok"
        if not self.expect(c, "the app asked for the image the page added", started):
            return
        t_img = self.fixture.rows("/slowimg/", mark)[0][0]
        loaded = self.app.wait_log(r"Loaded images added by live page scripts", 8, log_from)
        time.sleep(0.5)
        gap, count = self.widest_gap("beacon?img-tick-", t_img, t_img + 3.0)
        self.expect(c, "the ticks keep arriving while the image is out", gap <= 0.6,
                    "%d ticks in the 3 s it was out, widest gap %.2f s" % (count, gap))
        self.expect(c, "the engine logged the image as loaded by the live loop", bool(loaded))
        time.sleep(1.0)
        frame = self.frame(c, "loaded", [CYAN])
        if frame is None:
            return self.not_run(c, "the window shows the slow image", self.why_not("Screen Recording"))
        self.expect(c, "the window shows the slow image", frame[CYAN] > 2000, str(frame))

    def h1_slow(self):
        """H1: a request that takes three seconds neither stops the page's clock nor fails."""
        c = "h1_slow"
        if not self.open_page(c, "h1_slow.html"):
            return
        self.app.wait_log(r"Navigation finished", 20)
        time.sleep(2.5)

        now = time.time() - self.fixture.t0
        gap, count = self.widest_gap("beacon?slow-tick-", now - 2.0, now)
        self.expect(c, "with nothing pending, the 200 ms ticks arrive steadily", gap <= 0.6,
                    "%d ticks in 2 s, widest gap %.2f s" % (count, gap))
        mark = self.fixture.mark()
        self.fixture.reply = b"go"
        started = self.fixture.wait("/slow?", 4, mark)
        self.fixture.reply = b"ok"
        if not self.expect(c, "the page started the slow request", started):
            return
        t_slow = self.fixture.rows("/slow?", mark)[0][0]
        self.fixture.wait("beacon?slow-then", 6, mark)
        time.sleep(0.5)
        gap, count = self.widest_gap("beacon?slow-tick-", t_slow, t_slow + 3.0)
        self.expect(c, "the ticks keep arriving while the request is out", gap <= 0.6,
                    "%d ticks in the 3 s it was out, widest gap %.2f s" % (count, gap))
        caught = [urllib.parse.unquote(p) for p in self.fixture.seen("beacon?slow-catch", mark)]
        outcome = ["%s at +%.1f s" % (urllib.parse.unquote(p).split("?")[1], t - t_slow)
                   for (t, p) in self.fixture.rows("beacon?slow-", mark) if "tick" not in p]
        self.expect(c, "the three-second request reached its then()",
                    bool(self.fixture.seen("beacon?slow-then", mark)) and not caught, "; ".join(outcome) or "neither then nor catch")

    def h4(self):
        """H4: every image on a page appears, not only the first."""
        c = "h4"
        colours = [RED, GREEN, BLUE, MAGENTA, CYAN]
        if not self.open_page(c, "h4_images.html"):
            return
        self.fixture.wait("/img/", 10, self.since, count=5)
        asked = sorted(set(self.fixture.seen("/img/", self.since)))
        self.expect(c, "the app asked for all five images", len(asked) == 5, "%d: %s" % (len(asked), asked))
        m = self.app.wait_log(r"count=(\d+) Loaded images|Loaded images.*?count=(\d+)", 5)
        self.expect(c, "the engine logged five loaded images",
                    bool(m) and "5" in m.groups(), m.group(0) if m else "no 'Loaded images' line")
        time.sleep(1.0)
        frame = self.frame(c, "loaded", colours)
        if frame is None:
            return self.not_run(c, "the window shows all five images", self.why_not("Screen Recording"))
        missing = [("%02x%02x%02x" % col) for col in colours if frame[col] < 2000]
        self.expect(c, "the window shows all five images", not missing, "missing %s; %s" % (missing, frame))

    def h2(self):
        """H2: a resized window keeps CSS pixels the same size and relays out."""
        c = "h2"
        if not self.open_page(c, "h2_resize.html"):
            return
        self.fixture.wait("beacon?h2-width-", 10, self.since)
        widths = self.fixture.seen("beacon?h2-width-", self.since)
        self.expect(c, "the page reported its width", bool(widths),
                    "%s; window %s" % (widths, json.dumps(self.window)))
        # The side panel opening after the load narrows the content view by the
        # same call a window resize makes (apply_layout -> set_bounds).
        self.fixture.wait("beacon?h2-resize-", 5, self.since)
        heard = [p.rsplit("-", 1)[1] for p in self.fixture.seen("beacon?h2-resize-", self.since)]
        self.expect(c, "the page heard resize when the side panel took its width",
                    bool(heard) and bool(widths) and int(heard[-1]) < int(widths[0].rsplit("-", 1)[1]),
                    "innerWidth at resize: %s" % heard)
        if not self.can_resize:
            return self.not_run(c, "resize: fixed box keeps its size, half-width box follows",
                                self.why_not("Accessibility"))
        before = self.frame(c, "before", [MAGENTA, CYAN])
        for (w, h) in [(900, 600), (1500, 900)]:
            mark = self.fixture.mark()
            code, out = self.hw("resize", self.app.proc.pid, w, h)
            self.expect(c, "the window took the size %dx%d" % (w, h), code == 0, json.dumps(out))
            got = self.fixture.wait("beacon?h2-resize-", 5, mark)
            seen = self.fixture.seen("beacon?h2-resize-", mark)
            self.expect(c, "the page heard resize at %dx%d" % (w, h), got, str(seen[-1:]))
            time.sleep(1.0)
            after = self.frame(c, "at-%dx%d" % (w, h), [MAGENTA, CYAN])
            if before is None or after is None:
                self.not_run(c, "frames at %dx%d" % (w, h), self.why_not("Screen Recording"))
                continue
            ratio = after[MAGENTA] / max(before[MAGENTA], 1)
            self.expect(c, "the 200x100 box is the same size at %dx%d" % (w, h), 0.9 < ratio < 1.1,
                        "area ratio %.2f (before %s, after %s)" % (ratio, before, after))
            want = after["size"][0] / max(before["size"][0], 1)
            got_ratio = after[CYAN] / max(before[CYAN], 1)
            # The half-width box follows the content width, which is the window
            # less fixed side panels: same direction, not the same ratio.
            self.expect(c, "the 50%% box follows the window at %dx%d" % (w, h),
                        (got_ratio - 1) * (want - 1) > 0,
                        "window width ratio %.2f, box area ratio %.2f" % (want, got_ratio))

    def h3(self):
        """H3: hovering and pressing restyle and repaint."""
        c = "h3"
        if not self.open_page(c, "h3_hover.html"):
            return
        if not self.can_input:
            return self.not_run(c, "hover and press: script events, :hover and :active paint",
                                self.why_not("Accessibility"))
        pid = self.app.proc.pid
        x, y = self.centre()
        rest = self.frame(c, "rest", [RED, GREEN, BLUE])
        mark = self.fixture.mark()
        self.hw("move", pid, x - 40, y - 40)
        self.hw("move", pid, x, y)
        self.expect(c, "script heard mouseover", self.fixture.wait("beacon?h3-mouseover", 4, mark))
        time.sleep(0.6)
        hover = self.frame(c, "hover", [RED, GREEN, BLUE])
        self.hw("press", pid, x, y)
        self.expect(c, "script heard mousedown", self.fixture.wait("beacon?h3-mousedown", 4, mark))
        time.sleep(0.6)
        active = self.frame(c, "active", [RED, GREEN, BLUE])
        self.hw("release", pid, x, y)
        self.expect(c, "script heard click", self.fixture.wait("beacon?h3-click", 4, mark))
        if rest is None or hover is None or active is None:
            return self.not_run(c, ":hover and :active paint", self.why_not("Screen Recording"))
        self.expect(c, "at rest the target is red", rest[RED] > rest[GREEN] + rest[BLUE], str(rest))
        self.expect(c, ":hover paints green", hover[GREEN] > hover[RED], str(hover))
        self.expect(c, ":active paints blue", active[BLUE] > active[RED], str(active))

    def e19_icon_hover(self):
        """CH2.E19: an inline svg icon whose fill an author `:hover` rule sets turns from red to green under the pointer."""
        c = "e19_icon_hover"
        if not self.open_page(c, "e19_icon_hover.html"):
            return
        if not self.can_input:
            return self.not_run(c, "hover an icon: its svg fill follows `button:hover svg path`",
                                self.why_not("Accessibility"))
        pid = self.app.proc.pid
        x, y = self.centre()
        rest = self.frame(c, "rest", [RED, GREEN])
        mark = self.fixture.mark()
        self.hw("move", pid, x - 40, y - 40)
        self.hw("move", pid, x, y)
        self.expect(c, "script heard mouseover", self.fixture.wait("beacon?e19-mouseover", 4, mark))
        time.sleep(0.6)
        hover = self.frame(c, "hover", [RED, GREEN])
        self.hw("move", pid, x, 12)
        time.sleep(0.6)
        left = self.frame(c, "left", [RED, GREEN])
        if rest is None or hover is None or left is None:
            return self.not_run(c, "the icon's fill", self.why_not("Screen Recording"))
        self.expect(c, "at rest the icon is red", rest[RED] > 1000 and rest[GREEN] < rest[RED] / 10, str(rest))
        self.expect(c, "under the pointer the icon is green", hover[GREEN] > 1000 and hover[RED] < hover[GREEN] / 10,
                    str(hover))
        self.expect(c, "off the button it is red again", left[RED] > left[GREEN], str(left))

    def e22_focus_ring(self):
        """CH2.E22: a click in the RustKit chrome's address bar paints its `:focus-within` ring; Tab to a page field paints its `:focus` border."""
        c = "e22_focus_ring"
        if not self.open_page(c, "e22_focus_ring.html"):
            return
        self.fixture.wait("beacon?e22-ready", 10, self.since)
        self.app.wait_log(r'RustKit chrome IPC body=\{"cmd":"get_workspaces"', 20)
        time.sleep(1.0)
        if not self.can_input:
            return self.not_run(c, "click the address bar, Tab to a field", self.why_not("Accessibility"))
        if not self.can_capture:
            return self.not_run(c, "the ring and the border are painted", self.why_not("Screen Recording"))
        pid = self.app.proc.pid
        code, win = self.hw("window", pid)
        from PIL import Image

        def accent(name):
            """Pixels of the chrome's accent (#06b6d4) in the toolbar row."""
            path = self.out / ("%s.%s.png" % (c, name))
            code, w = self.hw("window", pid)
            subprocess.run(["/usr/sbin/screencapture", "-x", "-o", "-l", str(w.get("id", 0)), str(path)])
            img = Image.open(path).convert("RGB")
            sx = img.width / float(w["w"])
            # The toolbar row: window y 80..124 (32 px title bar), right of the sidebar.
            band = img.crop((int(230 * sx), int(80 * sx), img.width - int(10 * sx), int(124 * sx)))
            want = (6, 182, 212)
            return sum(n for n, rgb in band.getcolors(maxcolors=band.width * band.height)
                       if all(abs(rgb[i] - want[i]) <= 24 for i in range(3)))

        # The address bar: right of the sidebar and the nav buttons, in the
        # toolbar row (as a2_chrome_typed_url clicks it). Away from it first.
        self.hw("move", pid, int(win["w"] * 0.5), int(win["h"] * 0.9))
        time.sleep(0.6)
        rest = accent("bar-rest")
        log_from = len(self.app.log_text())
        self.hw("click", pid, int(win["w"] * 0.55), 100)
        clicked = self.app.wait_log(r"chrome click \(view-local\)", 3, log_from)
        self.expect(c, "the click reached the RustKit chrome", bool(clicked))
        self.hw("move", pid, int(win["w"] * 0.5), int(win["h"] * 0.9))
        time.sleep(0.8)
        focused = accent("bar-focused")
        self.expect(c, "the focused address bar paints its accent border", focused > rest + 300,
                    "accent pixels %d at rest, %d focused" % (rest, focused))
        # The page: a click on its blank half gives it the keys and focuses
        # nothing; Tab focuses the field, whose border turns green.
        self.hw("click", pid, int(win["w"] * 0.5), int(win["h"] * 0.9))
        time.sleep(0.6)
        before = self.frame(c, "field-rest", [RED, GREEN])
        mark = self.fixture.mark()
        self.hw("key", pid, 48)
        self.expect(c, "Tab focused the field", self.fixture.wait("beacon?e22-focus", 4, mark))
        time.sleep(0.6)
        tabbed = self.frame(c, "field-tabbed", [RED, GREEN])
        if before is None or tabbed is None:
            return self.not_run(c, "the field's border", self.why_not("Screen Recording"))
        self.expect(c, "at rest the field's border is red", before[RED] > 10000 and before[GREEN] < before[RED] / 10,
                    str(before))
        self.expect(c, "after Tab the field's border is green (`:focus`)",
                    tabbed[GREEN] > 10000 and tabbed[RED] < tabbed[GREEN] / 10, str(tabbed))

    def h6(self):
        """H6: the wheel scrolls the page."""
        c = "h6"
        if not self.open_page(c, "h6_scroll.html"):
            return
        if not self.can_input:
            return self.not_run(c, "wheel: the app sees it, script hears scroll, the frame moves",
                                self.why_not("Accessibility"))
        pid = self.app.proc.pid
        x, y = self.centre()
        top = self.frame(c, "top", [RED, GREEN, BLUE])
        mark, log_from = self.fixture.mark(), len(self.app.log_text())
        for _ in range(8):
            self.hw("scroll", pid, x, y, 0, -250)
        self.expect(c, "the app got the wheel",
                    bool(self.app.wait_log(r"wheel burst started", 3, log_from)))
        self.expect(c, "script heard scroll", self.fixture.wait("beacon?h6-scroll", 4, mark))
        time.sleep(0.8)
        down = self.frame(c, "scrolled", [RED, GREEN, BLUE])
        if top is None or down is None:
            return self.not_run(c, "the frame moves", self.why_not("Screen Recording"))
        self.expect(c, "the first screen is the red band only", top[RED] > 1000 and top[GREEN] == 0, str(top))
        self.expect(c, "after the wheel the green band is on screen", down[GREEN] > 1000, str(down))

    def h6_extent(self):
        """H6: a page whose content overflows a viewport-tall body can be scrolled to its end."""
        c = "h6_extent"
        if not self.open_page(c, "h6_extent.html"):
            return
        self.fixture.wait("beacon?h6x-reach-", 10, self.since)
        seen = self.fixture.seen("beacon?h6x-reach-", self.since)
        m = re.search(r"h6x-reach-(-?\d+)-of-(-?\d+)", seen[0]) if seen else None
        self.expect(c, "the page reported how far it scrolls", bool(m), str(seen))
        if not m:
            return
        reach, whole = int(m.group(1)), int(m.group(2))
        self.expect(c, "script scrolls to the end of the overflowing content",
                    whole > 0 and abs(reach - whole) <= 1, "scrollY %d of %d" % (reach, whole))
        time.sleep(0.8)
        frame = self.frame(c, "end", [RED, GREEN, BLUE])
        if frame is None:
            return self.not_run(c, "the window shows the last band", self.why_not("Screen Recording"))
        # The chrome has red pixels of its own (the Shield icon, the close
        # button: 53 in the 2026-10-06 run), so "no red" means "no red band".
        self.expect(c, "the window shows the last band", frame[BLUE] > 1000 and frame[RED] < 500, str(frame))

    def h8(self):
        """H8: a server's error page (a 403 with a body) is shown as the page."""
        c = "h8"
        if not self.open_page(c, "h8_error.html"):
            return
        self.expect(c, "the error page's script ran", self.fixture.wait("beacon?h8-ran", 10, self.since))
        self.expect(c, "the app asked for the error page's image",
                    self.fixture.wait("/img/00ff00.png", 5, self.since))
        m = self.app.wait_log(r"Showing the body of an error response", 2)
        self.expect(c, "the engine logged that it showed an error response's body", bool(m),
                    "" if m else "no such line; 'HTTP error' in the log: %s" % ("HTTP error" in self.app.log_text()))
        time.sleep(1.0)
        frame = self.frame(c, "loaded", [GREEN])
        if frame is None:
            return self.not_run(c, "the window shows the error page's green image", self.why_not("Screen Recording"))
        self.expect(c, "the window shows the error page's green image", frame[GREEN] > 2000, str(frame))

    def h16_click_nav(self):
        """H14/H16: a navigation that starts on a loaded page (a click, or the page's own script) shows the next page."""
        c = "h16_click_nav"
        # (route, how the driver says it, what a person would call it)
        routes = [("link", None, "a click on a link"),
                  ("click", b"click", "script: link.click()"),
                  ("assign", b"assign", "script: location.href = url"),
                  ("button", b"button", "script: submit button .click()"),
                  ("submit", b"submit", "script: form.submit()")]
        for (route, command, label) in routes:
            if not self.open_page(c, "h16_a.html"):
                self.close_page()
                continue
            self.app.wait_log(r"Navigation finished", 20)
            self.fixture.wait("beacon?h16-a-tick-", 5, self.since)
            time.sleep(0.5)
            first = self.frame(c, route + ".a", [RED, GREEN])
            mark, log_from = self.fixture.mark(), len(self.app.log_text())
            if command is None:
                if not self.can_input:
                    self.not_run(c, "%s: request, load, layout, script and frame of page B" % label,
                                 self.why_not("Accessibility"))
                    self.close_page()
                    continue
                x, y = self.centre()
                self.hw("move", self.app.proc.pid, x, y)
                self.hw("press", self.app.proc.pid, x, y)
                self.hw("release", self.app.proc.pid, x, y)
                self.expect(c, "%s: the app logged the click as a link" % label,
                            bool(self.app.wait_log(r"Link clicked", 3, log_from)))
            else:
                self.fixture.reply = command
                started = self.fixture.wait("beacon?h16-a-start-" + route, 4, mark)
                self.fixture.reply = b"ok"
                if not self.expect(c, "%s: page A started it" % label, started):
                    self.close_page()
                    continue
            # The four stages, in order; the first missing one is the finding.
            asked = self.fixture.wait("/h16_b.html", 6, mark)
            self.expect(c, "%s: the app requested page B" % label, asked,
                        str(self.fixture.seen("/h16_b.html", mark)[:1]))
            loading = self.app.wait_log(r"Loading URL[^\n]*h16_b\.html", 3 if asked else 0.5, log_from)
            self.expect(c, "%s: the engine logged Loading URL for page B" % label, bool(loading))
            built = self.app.wait_log(r"Root box built total_children=(\d+)", 5 if loading else 0.5,
                                      log_from + (loading.end() if loading else 0))
            self.expect(c, "%s: page B was laid out with boxes" % label,
                        bool(loading) and bool(built) and int(built.group(1)) > 0,
                        built.group(0) if built else "no layout after the load began")
            ran = self.fixture.wait("beacon?h16-b-ran", 5 if asked else 0.5, mark)
            self.expect(c, "%s: page B's script ran" % label, ran,
                        str(self.fixture.seen("beacon?h16-b-ran", mark)[:1]))
            time.sleep(1.0)
            frame = self.frame(c, route + ".b", [RED, GREEN])
            if first is None or frame is None:
                self.not_run(c, "%s: the window shows page B's green" % label, self.why_not("Screen Recording"))
            else:
                self.expect(c, "%s: the window showed page A's red before" % label, first[RED] > 1000, str(first))
                self.expect(c, "%s: the window shows page B's green" % label,
                            frame[GREEN] > 1000 and frame[RED] < 500, str(frame))
            (self.out / ("%s.%s.app.log" % (c, route))).write_text(self.app.log_text())
            self.close_page()
        # A page that sends itself on while it is being parsed, as a redirect
        # stub does: the request is made before the load has finished.
        label = "script: location.replace() in an inline script"
        if self.open_page(c, "h16_redirect.html"):
            asked = self.fixture.wait("/h16_b.html?from=inline", 8, self.since)
            self.expect(c, "%s: the app requested page B" % label, asked,
                        str(self.fixture.seen("/h16_b.html", self.since)[:1]))
            ran = self.fixture.wait("beacon?h16-b-ran-from=inline", 5 if asked else 0.5, self.since)
            self.expect(c, "%s: page B's script ran" % label, ran)
            again = len(self.fixture.seen("/h16_b.html", self.since))
            time.sleep(1.0)
            self.expect(c, "%s: page B was requested once" % label,
                        again == 1 and len(self.fixture.seen("/h16_b.html", self.since)) == 1,
                        "%d requests" % len(self.fixture.seen("/h16_b.html", self.since)))
            frame = self.frame(c, "inline.b", [RED, GREEN])
            if frame is None:
                self.not_run(c, "%s: the window shows page B's green" % label, self.why_not("Screen Recording"))
            else:
                self.expect(c, "%s: the window shows page B's green" % label,
                            frame[GREEN] > 1000 and frame[RED] < 500, str(frame))
            (self.out / ("%s.inline.app.log" % c)).write_text(self.app.log_text())
        self.close_page()

    # The engine's line for a script the budget stopped (load or live turn).
    STOP_LINE = r"Script stopped at the script budget source=(\S+) elapsed_ms=(\d+) late_ms=(\d+)"

    def h19_spin(self):
        """H19: a script that does not end is stopped at the app's script budget (15 s) and the page is shown."""
        c = "h19_spin"
        if not self.open_page(c, "h19_spin.html"):
            return
        self.expect(c, "the app asked for the page's image", self.fixture.wait("/img/00ff00.png", 10, self.since))
        # The app's start page finishes a navigation of its own first.
        running = self.app.wait_log(r"Running page script", 10)
        self.expect(c, "the engine started the page's script", bool(running))
        if not running:
            return
        # The page's own stop, then its load's end. Not the first
        # `Navigation finished`: the RustKit chrome loads its own page beside
        # this one, on its own thread, and writes that line too (batch 44:
        # 0 s after the page's script started, 15 s before its stop).
        started = time.time()
        stopped = self.app.wait_log(self.STOP_LINE, 45, running.end())
        m = stopped and self.app.wait_log(r"Navigation finished", 5, running.end() + stopped.end())
        took = time.time() - started
        self.expect(c, "the load finished within 45 s of the script's start", bool(m),
                    "%.0f s; app alive: %s" % (took, self.app.alive()))
        if not m:
            return
        # How long the script ran is the engine's own figure: the page runs
        # on the content thread, and the wall clock here also counts the
        # driver's polling.
        ran = int(stopped.group(2)) / 1000
        self.expect(c, "the script was given its budget before it was stopped", ran > 10, "%.1f s" % ran)
        # The page's thread is held while the script runs, so the budget is
        # also the longest a page can hold it: it must not creep back up.
        self.expect(c, "the window was not held much past the 15 s budget", ran < 25, "%.1f s" % ran)
        # The log says so itself: which script, how long it ran.
        self.expect(c, "the app log names the stopped script and how long it ran",
                    stopped.group(1).startswith("inline#") and 10000 < int(stopped.group(2)) < 25000,
                    stopped.group(0))
        time.sleep(1.0)
        frame = self.frame(c, "after", [GREEN])
        if frame is None:
            return self.not_run(c, "the window shows the page's green image", self.why_not("Screen Recording"))
        self.expect(c, "the window shows the page's green image", frame[GREEN] > 2000, str(frame))

    # ── I0(a): the user keeps control while a page misbehaves ───────────────
    #
    # Two measures. The app's own stall log (HIWAVE_STALL_LOG=1) says how long
    # the window thread went without turning its event loop; it needs nothing
    # from the seat. The controls themselves (a new tab, a resize, closing the
    # tab) need input and an unlocked screen, and the stall log does not stand
    # in for them: a loop that turns can still draw nothing.

    LIMIT_MS = 200
    STALL_ENV = {"HIWAVE_STALL_LOG": "1"}

    def stall_log_on(self, c):
        """The offset in the app log where the stall log starts, or None."""
        m = self.app.wait_log(r"Window thread stall log on", 5)
        if not m:
            self.not_run(c, "the window thread's holds", "this build writes no stall log (HIWAVE_STALL_LOG)")
            return None
        return m.end()

    def holds(self, log_from):
        """Every hold the stall log reported after `log_from`, in ms. A hold
        still going on is its latest 'holding' figure."""
        text = self.app.log_text()[log_from:]
        held = [int(ms) for ms in re.findall(r"Window thread held ms=(\d+)", text)]
        tail = text[text.rfind("Window thread held ms="):] if held else text
        going = [int(ms) for ms in re.findall(r"Window thread holding ms=(\d+)", tail)]
        return held + going[-1:]

    def expect_no_hold(self, c, during, log_from):
        if log_from is None:
            return
        holds = self.holds(log_from)
        self.expect(c, "%s: the window thread was never held %d ms or more" % (during, self.LIMIT_MS),
                    not holds, "holds (ms): %s" % (holds or "none"))

    def expect_controls(self, c, during, close=False, new_tab=True, frame="info"):
        """A resize, a new tab and (if asked) closing the tab, each answered
        within LIMIT_MS.

        The resize is answered when the window server shows the new size, as
        `hwdrive resize` times it itself (ack_ms); until T36 this timed the
        whole hwdrive subprocess, which ended in a fixed 400 ms sleep and so
        could never pass (TRIAGE-i0a-2026-11-09). The page view's first frame
        laid out at the new size (its first `relayout cause=resize` in the
        app log, timed from hwdrive's own request time) is asserted against
        the same limit when `frame="assert"` (i0a_layout) and reported for
        information otherwise: a runaway script or a load in progress holds
        the page's thread, and that is LAT15/LAT17's to fix, not a resize
        bug.

        The resize comes first: Cmd+T loads the start page into the same
        content view, and the frame must be the page under test's.

        `new_tab=False` leaves out Cmd+T: a check that goes on measuring the
        page cannot open one (i0a_script, i0a_nav; i0a_layout and i0a_close
        keep it)."""
        if not self.can_input:
            for what in ["a resize"] + (["a new tab (Cmd+T)"] if new_tab else []) + (["closing the tab (Cmd+W)"] if close else []):
                self.not_run(c, "%s: %s is answered within %d ms" % (during, what, self.LIMIT_MS),
                             self.why_not("Accessibility"))
            if frame == "assert":
                self.not_run(c, "%s: the page is laid out at the new size within %d ms" % (during, self.LIMIT_MS),
                             self.why_not("Accessibility"))
            return
        pid, limit = self.app.proc.pid, self.LIMIT_MS / 1000

        def key(code, pattern, what):
            log_from, t0 = len(self.app.log_text()), time.time()
            self.hw("key", pid, code, "cmd")
            m = self.app.wait_log(pattern, 3, log_from)
            took = time.time() - t0
            self.expect(c, "%s: %s is answered within %d ms" % (during, what, self.LIMIT_MS),
                        bool(m) and took < limit, "%.0f ms" % (took * 1000) if m else "no log line in 3 s")

        if self.can_resize:
            self.expect_resize(c, during, pid, frame)
        else:
            self.not_run(c, "%s: a resize is answered within %d ms" % (during, self.LIMIT_MS),
                         self.why_not("Accessibility"))
            if frame == "assert":
                self.not_run(c, "%s: the page is laid out at the new size within %d ms" % (during, self.LIMIT_MS),
                             self.why_not("Accessibility"))
        if new_tab:
            key(17, r"New tab - navigating to start page", "a new tab (Cmd+T)")
        if close:
            key(13, r"Closed tab", "closing the tab (Cmd+W)")

    def expect_resize(self, c, during, pid, frame):
        code, win = self.hw("window", pid)
        w, h = int(win.get("w", 1200)) - 120, int(win.get("h", 800)) - 80
        t0 = time.time()
        code, out = self.hw("resize", pid, w, h)
        took = time.time() - t0
        # The answer is hwdrive's own figure (request to the window server
        # showing the new size). The subprocess's wall time also counts its
        # spawn and window lookup, and it is kept for information only.
        ack, ax = resize_ack(out)
        has_ack = isinstance(out, dict) and "ack_ms" in out
        code2, now = self.hw("window", pid)
        # The page view's first frame laid out at the new size, timed from
        # hwdrive's request (req_epoch), not from before hwdrive was spawned.
        req = resize_request_epoch(out)
        view = page_view_id(self.app.log_text(), self.fixture.base)
        wait = 2.0 if frame == "assert" else 0.3
        end = time.time() + wait
        laid = resize_relayouts(self.app.log_text(), req) if req is not None else {}
        while req is not None and view is not None and view not in laid and time.time() < end:
            time.sleep(0.05)
            laid = resize_relayouts(self.app.log_text(), req)
        ms = laid.get(view) if view is not None else None
        others = ", ".join("view %d +%.0f ms" % kv for kv in sorted(laid.items()) if kv[0] != view)
        detail = "%s; page view %s%s" % (
            "%.0f ms after the request" % ms if ms is not None
            else "no req_epoch from hwdrive" if req is None
            else "no fixture load in the log" if view is None
            else "no relayout cause=resize in %.1f s" % wait,
            view if view is not None else "?", "; other views (info): " + others if others else "")
        ack_detail = ("ack %s (AX set %s); subprocess wall %.0f ms; asked %dx%d, window %s"
                      % ("%.1f ms" % ack if ack is not None else "none in 2 s" if has_ack else "not reported",
                         "%.1f ms" % ax if ax is not None else "-", took * 1000, w, h, json.dumps(now)))
        if frame != "assert":
            ack_detail += "; info, not asserted: the page's first frame at the new size: " + detail
        self.expect(c, "%s: a resize is answered within %d ms" % (during, self.LIMIT_MS),
                    code == 0 and ack is not None and ack < self.LIMIT_MS and abs(now.get("w", 0) - w) < 4,
                    ack_detail)
        if frame == "assert":
            self.expect(c, "%s: the page is laid out at the new size within %d ms" % (during, self.LIMIT_MS),
                        ms is not None and ms < self.LIMIT_MS, detail)

    def start_spin(self, c):
        """Opens the page whose timer starts an endless script and returns the
        stall-log offset once the script is running, or None."""
        if not self.open_page(c, "i0a_spin.html", self.STALL_ENV):
            return None
        log_from = self.stall_log_on(c)
        # Ticks after the load: the page is live and its timers run.
        ticking = self.fixture.wait("beacon?i0a-spin-tick-", 15, self.since, count=5)
        self.expect(c, "the loaded page ticks", ticking)
        if not ticking:
            return None
        self.expect_no_hold(c, "before the script starts", log_from)
        self.fixture.reply = b"spin"
        armed = self.fixture.wait("beacon?i0a-spin-armed", 5, self.since)
        self.fixture.reply = b"ok"
        self.expect(c, "the page said its script is about to start", armed)
        if not armed:
            return None
        time.sleep(1.5)  # the page starts it one second after saying so
        return len(self.app.log_text()) if log_from is not None else None

    def i0a_script(self):
        """I0(a): a script that does not end leaves the window usable, is stopped at the turn budget (5 s), and the page carries on."""
        c = "i0a_script"
        log_from = self.start_spin(c)
        if self.app is None or not self.app.alive():
            return
        spin_from = len(self.app.log_text())
        # No Cmd+T here: the new tab would take the content view this check
        # goes on measuring (batch 44: its start page loaded the moment the
        # stop freed the page's thread). i0a_close presses it during the
        # same spin.
        self.expect_controls(c, "while the script runs", new_tab=False)
        time.sleep(5)
        self.expect_no_hold(c, "5 s of the script running", log_from)
        # The page's script runs on the content thread, so the window thread
        # is never held and its stall log has no hold to end: the hold that
        # ends is the page thread's, at the live turn budget (5 s), and the
        # engine says so.
        stopped = self.app.wait_log(self.STOP_LINE, 5, spin_from)
        ended = stopped if stopped and stopped.group(1) == "timers" else None
        self.expect(c, "the hold ended within 10 s of the script's start", bool(ended),
                    ended.group(0) if ended else "no 'Script stopped at the script budget source=timers' line")
        mark = self.fixture.mark()
        self.expect(c, "the page ticks again afterwards",
                    bool(ended) and self.fixture.wait("beacon?i0a-spin-tick-", 3, mark))
        asked = self.fixture.seen("beacon?i0a-spin-asked", self.since)
        self.expect(c, "a request made in the turn that was stopped is answered to the page",
                    bool(asked) and self.fixture.wait("beacon?i0a-spin-answered", 3, self.since),
                    "asked %d, onload %d" % (len(asked), len(self.fixture.seen("beacon?i0a-spin-answered", self.since))))
        frame = self.frame(c, "during", [GREEN])
        if frame is None:
            return self.not_run(c, "the window still shows the page's green image", self.why_not("Screen Recording"))
        self.expect(c, "the window still shows the page's green image", frame[GREEN] > 2000, str(frame))

    def i0a_layout(self):
        """I0(a): a page whose layout is very expensive leaves the window usable."""
        c = "i0a_layout"
        if not self.open_page(c, "i0a_layout.html?n=150", self.STALL_ENV):
            return
        log_from = self.stall_log_on(c)
        built = self.fixture.wait("beacon?i0a-layout-built-", 30, self.since)
        self.expect(c, "the page built its boxes", built)
        turned = self.fixture.wait("beacon?i0a-layout-turn-", 60, self.since, count=6)
        self.expect(c, "the page changed every box's width six times", turned,
                    "%d turns" % len(self.fixture.seen("beacon?i0a-layout-turn-", self.since)))
        # The one I0(a) page whose thread is not knowingly held: its first
        # frame at the new size is asserted, not only reported.
        self.expect_controls(c, "while the page lays out", frame="assert")
        self.expect_no_hold(c, "the load and six layouts", log_from)

    def i0a_nav(self):
        """I0(a): pages left while they are still loading leave the window usable."""
        c = "i0a_nav"
        if not self.open_page(c, "i0a_nav.html?n=1", self.STALL_ENV):
            return
        log_from = self.stall_log_on(c)
        # No Cmd+T here, as in i0a_script: the new tab's start page goes into
        # the content view this check goes on measuring, and in batch 47 it
        # cancelled page 1 before its first script ran, so no visit, no
        # leaving and no green frame could be seen (TRIAGE-i0a-2026-11-09).
        # i0a_layout and i0a_close still press it.
        self.expect_controls(c, "while the first page loads", new_tab=False)
        last = self.fixture.wait("beacon?i0a-nav-last", 40, self.since)
        starts = [p.rsplit("-", 1)[1] for p in self.fixture.seen("beacon?i0a-nav-start-", self.since)]
        self.expect(c, "the five visits happened in order and the last one finished", last and starts == list("12345"),
                    "visits started: %s; request times (s): %s"
                    % (starts, self.fixture.times("/i0a_nav.html", self.since)))
        # Each page asks to leave in its first script. How long it stays is
        # how long a person's own click on a link would wait, too.
        asked = self.fixture.times("/i0a_nav.html", self.since)
        stays = [round(b - a, 1) for (a, b) in zip(asked, asked[1:])]
        self.expect(c, "each page was left within 1 s of being requested", len(stays) == 4 and max(stays) < 1.0,
                    "seconds each page stayed: %s" % stays)
        self.expect_no_hold(c, "five pages, four left while loading", log_from)
        time.sleep(1.0)
        frame = self.frame(c, "last", [RED, GREEN])
        if frame is None:
            return self.not_run(c, "the window shows the last page's green image", self.why_not("Screen Recording"))
        self.expect(c, "the window shows the last page's green image", frame[GREEN] > 2000 and frame[RED] < 500, str(frame))

    def i0a_close(self):
        """I0(a): a tab whose script does not end can be closed at once."""
        c = "i0a_close"
        self.start_spin(c)
        if self.app is None or not self.app.alive():
            return
        self.expect_controls(c, "while the script runs", close=True)
        if not self.can_input:
            return
        # With the tab gone its script must stop: the stall log goes quiet.
        time.sleep(2.0)
        log_from = len(self.app.log_text())
        time.sleep(2.0)
        self.expect_no_hold(c, "2 s after the tab was closed", log_from)

    def e9_redirect(self):
        """CH2.E9: after a redirect the tab (which feeds the address bar) holds the committed URL and the title."""
        self.e9_redirect_on("e9_redirect")

    def e9_redirect_webkit(self):
        """CH2.E9 with HIWAVE_RUSTKIT_CHROME=0 (the WebKit chrome, CH3.A5a's escape hatch): the tab holds the committed URL and the title."""
        self.e9_redirect_on("e9_redirect_webkit", WEBKIT_CHROME)

    def e9_redirect_on(self, c, env=None):
        if not self.open_page(c, "e9_start", env=env):
            return
        self.expect(c, "the app followed the redirect", self.fixture.wait("/e9_final.html", 15, self.since))
        start = "%s/e9_start" % self.fixture.base
        final = "%s/e9_final.html" % self.fixture.base
        urls, text = [], ""
        end = time.time() + 15
        while time.time() < end:
            try:
                text = self.app.state_file.read_text()
                urls = tab_urls(json.loads(text))
            except (FileNotFoundError, ValueError):
                urls = []
            if final in urls and "E9 final" in text:
                break
            time.sleep(0.2)
        self.expect(c, "the saved tab holds the committed URL, not the one restored",
                    final in urls and start not in urls, "tab urls %s" % urls)
        self.expect(c, "the saved tab holds the page's title", "E9 final" in text)

    A6_SECRET = "a6-secret-4Kp"

    def a6_url_log(self):
        """CH3.A6: without HIWAVE_DEBUG/HIWAVE_TEST_HOOKS a page URL's query and fragment never reach the app log; its path does."""
        c = "a6_url_log"
        secret = self.A6_SECRET
        page = "e9_final.html?token=%s#%s" % (secret, secret)
        if not self.open_page(c, page, env={"HIWAVE_TEST_HOOKS": "0"}):
            return
        path = "%s/e9_final.html" % self.fixture.base
        nav = self.app.wait_log(r"Navigating content to: [^\n]*e9_final\.html[^\n]*", 10)
        self.expect(c, "the navigation line names the page's path",
                    bool(nav) and nav.group(0).rstrip().endswith(path), nav.group(0) if nav else "")
        time.sleep(1.0)
        log = self.app.log_text()
        self.expect(c, "the query and fragment are nowhere in the log", secret not in log,
                    "; ".join(l.strip()[-160:] for l in log.splitlines() if secret in l)[:600])

    def a2_chrome_boots(self):
        """CH3.A2/A5a: by default the chrome is a RustKit view whose script reaches the app."""
        c = "a2_chrome_boots"
        if not self.open_page(c, "e9_final.html"):
            return
        m = self.app.wait_log(r"the chrome is a RustKit view", 10)
        self.expect(c, "the app built the chrome as a RustKit view", bool(m))
        self.expect(c, "the startup line names the RustKit chrome",
                    bool(self.app.wait_log(r"Chrome: RustKit \(default", 1)))
        # What the WebKit chrome posts at boot: its sidebar width, its workspaces.
        tabs = self.app.wait_log(r'RustKit chrome IPC body=\{"cmd":"get_sidebar_width"', 20)
        self.expect(c, "the chrome's script posted get_sidebar_width and the app's handler took it", bool(tabs),
                    tabs.group(0) if tabs else "")
        spaces = self.app.wait_log(r'RustKit chrome IPC body=\{"cmd":"get_workspaces"', 5)
        self.expect(c, "the chrome's script posted get_workspaces", bool(spaces))
        time.sleep(3)
        self.expect(c, "the app is still running 3 s later", self.app.alive())

    def a7_no_bridge_in_content(self):
        """CH3.A7: a web page in the content view has no window.ipc / window.hiwaveChrome, with HIWAVE_RUSTKIT_CHROME=0 (flag off) and by default (flag on)."""
        c = "a7_no_bridge_in_content"
        for label, env in (("flag off", WEBKIT_CHROME), ("flag on", None)):
            if not self.open_page(c, "a7_bridge.html", env=env):
                self.close_page()
                continue
            got = self.fixture.wait("beacon?a7-bridge=", 15, self.since)
            seen = self.fixture.seen("a7-bridge=", self.since)
            self.expect(c, "%s: the page's script ran and reported" % label, bool(got))
            self.expect(c, "%s: the page sees typeof window.ipc, window.hiwaveChrome = undefined,undefined" % label,
                        any("a7-bridge=undefined%2Cundefined" in r for r in seen), str(seen))
            if label == "flag on":
                # The chrome, an app-owned view, keeps its bridge meanwhile.
                ipc = self.app.wait_log(r'RustKit chrome IPC body=\{"cmd":"get_sidebar_width"', 20)
                self.expect(c, "%s: the chrome still posts get_sidebar_width" % label, bool(ipc))
            self.close_page()

    def a2_chrome_navigate(self):
        """CH3.A2 3c: the address bar's navigate, run in the RustKit chrome, reaches the app and the page loads it."""
        c = "a2_chrome_navigate"
        target = "%s/h16_b.html?from=a2" % self.fixture.base
        # The call the address bar's Enter handler makes. Not hiwave.navigate():
        # chrome.html replaces the bridge's window.hiwave with its own object.
        env = {"HIWAVE_RUSTKIT_CHROME_EVAL": "sendIpc('navigate', { url: %s })" % json.dumps(target)}
        if not self.open_page(c, "e9_final.html", env=env):
            return
        ran = self.app.wait_log(r"running HIWAVE_RUSTKIT_CHROME_EVAL", 20)
        self.expect(c, "the app ran the script in the booted chrome", bool(ran))
        ipc = self.app.wait_log(r'RustKit chrome IPC body=\{"cmd":"navigate","url":"[^"]*h16_b\.html\?from=a2"', 10)
        self.expect(c, "the chrome posted navigate and the app's handler took it", bool(ipc),
                    ipc.group(0) if ipc else "")
        nav = self.app.wait_log(r"Navigating content to: [^\n]*h16_b\.html\?from=a2", 10)
        self.expect(c, "the app navigated the content view", bool(nav))
        self.expect(c, "the app requested the page", self.fixture.wait("/h16_b.html?from=a2", 10, self.since))
        self.expect(c, "the page's script ran in the content view",
                    self.fixture.wait("beacon?h16-b-ran-from=a2", 10, self.since))
        urls = []
        end = time.time() + 10
        while time.time() < end and target not in urls:
            try:
                urls = tab_urls(json.loads(self.app.state_file.read_text()))
            except (FileNotFoundError, ValueError):
                urls = []
            time.sleep(0.2)
        self.expect(c, "the saved tab holds the URL the chrome asked for", target in urls, "tab urls %s" % urls)

    def a2_chrome_typed_url(self):
        """CH3.A2 granted gate: RustKit chrome (the default), the tab strip and toolbar are painted, and a URL typed in the address bar navigates."""
        c = "a2_chrome_typed_url"
        if not self.open_page(c, "e9_final.html"):
            return
        self.app.wait_log(r'RustKit chrome IPC body=\{"cmd":"get_workspaces"', 20)
        time.sleep(1.0)
        if self.can_capture:
            time.sleep(0.8)
            path = self.out / ("%s.before.png" % c)
            code, win = self.hw("window", self.app.proc.pid)
            subprocess.run(["/usr/sbin/screencapture", "-x", "-o", "-l", str(win.get("id", 0)), str(path)])
            from PIL import Image
            img = Image.open(path).convert("RGB")
            # The window's own title bar is 32 px; the tab strip and toolbar
            # are the next ~100 px, right of the 220 px sidebar.
            band = img.crop((230, 36, img.width - 10, 128))
            colours = len(band.getcolors(maxcolors=band.width * band.height) or [])
            self.expect(c, "the tab strip and toolbar are painted (not one flat colour)", colours > 50,
                        "%d colours in the band" % colours)
        else:
            self.not_run(c, "the tab strip and toolbar are painted", self.why_not("Screen Recording"))
        if not self.can_input:
            return self.not_run(c, "a URL typed in the address bar navigates", self.why_not("Accessibility"))
        target = "%s/h16_b.html?from=typed" % self.fixture.base
        pid = self.app.proc.pid
        # A new tab's address bar is empty: typing over a URL already there
        # appends to it (select() and Cmd+A select nothing; the PR lists it).
        log_from = len(self.app.log_text())
        self.hw("key", pid, 17, "cmd")
        made = self.app.wait_log(r'RustKit chrome IPC body=\{"cmd":"create_tab"', 5, log_from)
        self.expect(c, "Cmd+T made a tab through the chrome", bool(made))
        time.sleep(1.0)
        code, win = self.hw("window", pid)
        # The address bar: right of the sidebar and the three nav buttons,
        # in the toolbar row (window y 100 with the 32 px title bar).
        self.hw("click", pid, int(win["w"] * 0.55), 100)
        clicked = self.app.wait_log(r"chrome click \(view-local\)", 3, log_from)
        self.expect(c, "the click reached the RustKit chrome", bool(clicked), clicked.group(0) if clicked else "")
        time.sleep(0.5)
        mark, log_from = self.fixture.mark(), len(self.app.log_text())
        self.hw("type", pid, target)
        self.hw("key", pid, 36)
        ipc = self.app.wait_log(r'RustKit chrome IPC body=\{"cmd":"navigate","url":"([^"]*)"', 10, log_from)
        self.expect(c, "Return posted navigate with the typed URL", bool(ipc) and ipc.group(1) == target,
                    ipc.group(1) if ipc else "no navigate")
        asked = self.fixture.wait("/h16_b.html?from=typed", 10, mark) and \
            "/h16_b.html?from=typed" in self.fixture.seen("/h16_b.html", mark)
        self.expect(c, "the content view requested the typed URL", asked)
        self.expect(c, "the page's script ran", self.fixture.wait("beacon?h16-b-ran-from=typed", 10, mark))
        time.sleep(1.0)
        frame = self.frame(c, "after", [GREEN])
        if frame is None:
            return self.not_run(c, "the window shows the typed page's green", self.why_not("Screen Recording"))
        self.expect(c, "the window shows the typed page's green", frame[GREEN] > 100000, str(frame))

    # ── CH2.E3: text input, the Edit menu, Cmd chords ───────────────────────

    def e3_start(self, c, page="e3_text.html", env=None):
        """A fresh app on the E3 page with input granted: (pid, w, h), or None."""
        if not self.open_page(c, page, env=env):
            return None
        if not self.can_input:
            self.not_run(c, "keys and menu items reach the page", self.why_not("Accessibility"))
            return None
        self.fixture.wait("beacon?e3-ready" if page.startswith("e3_text") else "beacon?e3n-ready", 10, self.since)
        pid = self.app.proc.pid
        code, win = self.hw("window", pid)
        # HID keys go to the frontmost app: never type into another one.
        # macOS may refuse `activate`; a click on the page's blank bottom
        # edge brings the window forward as a user's would.
        code, front = self.hw("activate", pid)
        if not front.get("frontmost"):
            self.hw("click", pid, win["w"] * 0.5, win["h"] - 12)
            time.sleep(0.4)
            code, front = self.hw("activate", pid)
        if not front.get("frontmost"):
            self.not_run(c, "keys reach the app", "the app is not frontmost: " + json.dumps(front))
            return None
        code, win = self.hw("window", pid)
        return pid, win["w"], win["h"]

    def e3_value(self, field, mark):
        """The value the page last reported for `field` since `mark`."""
        seen = [urllib.parse.unquote(p) for p in self.fixture.seen("beacon?e3-%s-v=" % field, mark)]
        return seen[-1].split("-v=", 1)[1].rsplit("-c=", 1)[0] if seen else None

    def e3_wait_value(self, field, mark, want, timeout=3):
        end = time.time() + timeout
        while time.time() < end and self.e3_value(field, mark) != want:
            time.sleep(0.1)
        return self.e3_value(field, mark)

    def e3_focus(self, c, pid, w, h, field="a"):
        mark = self.fixture.mark()
        self.hw("click", pid, w * 0.7, h * (0.35 if field == "a" else 0.75))
        ok = self.fixture.wait("beacon?e3-%s-focus" % field, 4, mark)
        self.expect(c, "#%s took the focus" % field, ok)
        time.sleep(0.4)
        return ok

    def pasteboard(self, text=None):
        """Read the general pasteboard, or replace it with `text`."""
        if text is None:
            return subprocess.run(["pbpaste"], capture_output=True, text=True).stdout
        subprocess.run(["pbcopy"], input=text, text=True)

    def e3_dead_key(self):
        """CH2.E3: Option+e then e in a content <input> composes é (compositionstart, compositionend é)."""
        c = "e3_dead_key"
        started = self.e3_start(c)
        if not started:
            return
        pid, w, h = started
        if not self.e3_focus(c, pid, w, h):
            return
        mark = self.fixture.mark()
        self.hw("key", pid, 14, "alt")  # kVK_ANSI_E with Option: the dead acute
        time.sleep(0.5)
        start = [urllib.parse.unquote(p) for p in self.fixture.seen("beacon?e3-a-compositionstart", mark)]
        self.expect(c, "Option+e starts a composition (compositionstart)", bool(start), str(start))
        self.hw("key", pid, 14)
        value = self.e3_wait_value("a", mark, "é")
        self.expect(c, "then e commits é (Chrome: é)", value == "é", repr(value))
        end = [urllib.parse.unquote(p) for p in self.fixture.seen("beacon?e3-a-compositionend", mark)]
        self.expect(c, "compositionend carries é", end[-1:] == ["/beacon?e3-a-compositionend=é"], str(end))

    def e3_dead_key_no_field(self):
        """CH2.E3 (#918 R1): with nothing editable focused, Option+e, e and j are plain keydowns; nothing composes or is inserted."""
        c = "e3_dead_key_no_field"
        started = self.e3_start(c, page="e3_no_field.html")
        if not started:
            return
        pid, w, h = started
        mark = self.fixture.mark()
        self.hw("key", pid, 14, "alt")  # Option+e: the dead acute on a US layout
        time.sleep(0.3)
        self.hw("key", pid, 14)  # e
        time.sleep(0.3)
        self.hw("key", pid, 38)  # j, a shortcut key
        time.sleep(1.0)
        seen = lambda kind: [urllib.parse.unquote(p).split("=", 1)[1]
                             for p in self.fixture.seen("beacon?e3n-%s=" % kind, mark)]
        downs, ups = seen("keydown"), seen("keyup")
        keys = [d.split("-", 1)[1] for d in downs]
        # Every key is a keydown the page hears, none swallowed by an input
        # method. Not pinned (#918 nit 5, no OS-key Chromium oracle on this
        # seat): Chromium's key values for the dead key itself ("Dead") and
        # the composed key, and keyCode (the engine reports 0 for character
        # keys); keyup with nothing focused is not delivered (as before E3).
        self.expect(c, "the page hears a keydown for each of Option+e, e, j", len(keys) == 3 and keys[-1] == "j",
                    str(downs))
        self.note(c, "keyups heard", "INFO", str(ups))
        composed = sum((seen(k) for k in ("compositionstart", "compositionupdate", "compositionend")), [])
        self.expect(c, "nothing composes", not composed, str(composed))
        inserted = seen("beforeinput") + seen("input")
        self.expect(c, "nothing is inserted", not inserted, str(inserted))

    def e3_paste(self):
        """CH2.E3: Cmd+V and Edit > Paste put the pasteboard's text in a content <input>."""
        c = "e3_paste"
        saved = self.pasteboard()
        try:
            started = self.e3_start(c)
            if not started:
                return
            pid, w, h = started
            if not self.e3_focus(c, pid, w, h):
                return
            self.pasteboard("e3-pasted")
            mark = self.fixture.mark()
            self.hw("key", pid, 9, "cmd")  # V
            value = self.e3_wait_value("a", mark, "e3-pasted")
            self.expect(c, "Cmd+V pastes the pasteboard's text", value == "e3-pasted", repr(value))
            code, out = self.hw("menu", pid, "Edit", "Paste")
            value = self.e3_wait_value("a", mark, "e3-pastede3-pasted")
            self.expect(c, "Edit > Paste pastes it again", value == "e3-pastede3-pasted",
                        "%r %s" % (value, json.dumps(out)))
        finally:
            self.pasteboard(saved)

    def e3_cut_copy(self):
        """CH2.E3: Cmd+A/C/X and Edit > Select All/Copy/Cut put the field's text on the pasteboard."""
        c = "e3_cut_copy"
        saved = self.pasteboard()
        try:
            started = self.e3_start(c)
            if not started:
                return
            pid, w, h = started
            if not self.e3_focus(c, pid, w, h):
                return
            mark = self.fixture.mark()
            self.hw("type", pid, "hello")
            self.e3_wait_value("a", mark, "hello")
            self.pasteboard("before")
            self.hw("key", pid, 0, "cmd")  # A
            time.sleep(0.3)
            self.hw("key", pid, 8, "cmd")  # C
            time.sleep(0.6)
            self.expect(c, "Cmd+A, Cmd+C copies the value", self.pasteboard() == "hello", repr(self.pasteboard()))
            self.hw("key", pid, 7, "cmd")  # X
            value = self.e3_wait_value("a", mark, "")
            self.expect(c, "Cmd+X empties the field", value == "", repr(value))
            mark = self.fixture.mark()
            self.hw("type", pid, "world")
            self.e3_wait_value("a", mark, "world")
            self.pasteboard("before")
            self.hw("menu", pid, "Edit", "Select All")
            time.sleep(0.3)
            self.hw("menu", pid, "Edit", "Copy")
            time.sleep(0.6)
            self.expect(c, "Edit > Select All, Edit > Copy copies the value", self.pasteboard() == "world",
                        repr(self.pasteboard()))
            self.hw("menu", pid, "Edit", "Cut")
            value = self.e3_wait_value("a", mark, "")
            self.expect(c, "Edit > Cut empties the field", value == "", repr(value))
        finally:
            self.pasteboard(saved)

    def e3_undo(self):
        """CH2.E3: Cmd+Z, Cmd+Shift+Z and Edit > Undo in a content <input>."""
        c = "e3_undo"
        started = self.e3_start(c)
        if not started:
            return
        pid, w, h = started
        if not self.e3_focus(c, pid, w, h):
            return
        mark = self.fixture.mark()
        self.hw("type", pid, "abc")
        self.e3_wait_value("a", mark, "abc")
        self.hw("key", pid, 6, "cmd")  # Z
        value = self.e3_wait_value("a", mark, "")
        self.expect(c, "Cmd+Z undoes the typing", value == "", repr(value))
        self.hw("key", pid, 6, "cmd", "shift")
        value = self.e3_wait_value("a", mark, "abc")
        self.expect(c, "Cmd+Shift+Z redoes it", value == "abc", repr(value))
        self.hw("menu", pid, "Edit", "Undo")
        value = self.e3_wait_value("a", mark, "")
        self.expect(c, "Edit > Undo undoes it", value == "", repr(value))

    def e3_cmd_key_to_page(self):
        """CH2.E3: a Cmd chord reaches the page's keydown before the menu; a cancelled one stays the page's. Cmd+T is Chromium-reserved: the page never hears it and the tab opens."""
        c = "e3_cmd_key_to_page"
        for prevent in (True, False):
            page = "e3_text.html" + ("?prevent" if prevent else "")
            started = self.e3_start(c, page)
            if not started:
                return
            pid, w, h = started
            if not self.e3_focus(c, pid, w, h):
                return
            label = "cancelled" if prevent else "not cancelled"
            mark, log_from = self.fixture.mark(), len(self.app.log_text())
            self.hw("key", pid, 3, "cmd")  # F
            heard = self.fixture.wait("beacon?e3-cmd=f", 3, mark)
            self.expect(c, "%s: the page hears Cmd+F's keydown (metaKey)" % label, heard)
            find = self.app.wait_log(r'menu event id="?find', 2, log_from)
            if prevent:
                self.expect(c, "cancelled: the menu does not open Find", not find)
            else:
                self.expect(c, "not cancelled: the menu opens Find", bool(find))
            self.hw("key", pid, 53)  # Escape closes the find bar
            time.sleep(0.4)
            mark, log_from = self.fixture.mark(), len(self.app.log_text())
            self.hw("key", pid, 17, "cmd")  # T
            tab = self.app.wait_log(r'menu event id="?new_tab', 3, log_from)
            self.expect(c, "%s: Cmd+T opens a tab (Chrome 143: reserved)" % label, bool(tab))
            heard = self.fixture.seen("beacon?e3-cmd=t", mark)
            self.expect(c, "%s: the page hears no Cmd+T keydown (Chrome 143)" % label, not heard, str(heard))
            self.close_page()

    def e3_chrome_paste_url(self):
        """CH2.E3, RustKit chrome (the default): Cmd+V pastes a URL into the RustKit chrome's address bar and Return navigates."""
        c = "e3_chrome_paste_url"
        saved = self.pasteboard()
        try:
            if not self.open_page(c, "e9_final.html"):
                return
            if not self.can_input:
                return self.not_run(c, "a pasted URL navigates", self.why_not("Accessibility"))
            self.app.wait_log(r'RustKit chrome IPC body=\{"cmd":"get_workspaces"', 20)
            time.sleep(1.0)
            pid = self.app.proc.pid
            log_from = len(self.app.log_text())
            self.hw("key", pid, 17, "cmd")  # a new tab: an empty address bar
            made = self.app.wait_log(r'RustKit chrome IPC body=\{"cmd":"create_tab"', 5, log_from)
            self.expect(c, "Cmd+T made a tab through the chrome", bool(made))
            time.sleep(1.0)
            code, win = self.hw("window", pid)
            self.hw("click", pid, int(win["w"] * 0.55), 100)
            clicked = self.app.wait_log(r"chrome click \(view-local\)", 3, log_from)
            self.expect(c, "the click reached the RustKit chrome", bool(clicked))
            time.sleep(0.5)
            target = "%s/h16_b.html?from=pasted" % self.fixture.base
            self.pasteboard(target)
            mark, log_from = self.fixture.mark(), len(self.app.log_text())
            self.hw("key", pid, 9, "cmd")  # V
            time.sleep(0.5)
            self.hw("key", pid, 36)
            ipc = self.app.wait_log(r'RustKit chrome IPC body=\{"cmd":"navigate","url":"([^"]*)"', 10, log_from)
            self.expect(c, "Return after Cmd+V posted navigate with the pasted URL",
                        bool(ipc) and ipc.group(1) == target, ipc.group(1) if ipc else "no navigate")
            self.expect(c, "the page's script ran", self.fixture.wait("beacon?h16-b-ran-from=pasted", 10, mark))
        finally:
            self.pasteboard(saved)

    # ── CH3.A3: the RustKit chrome over the page ───────────────────────────

    TITLE_BAR = 32  # the window's own title bar, above the chrome view

    def a3_open(self, c, env=None):
        """A fresh app, RustKit chrome (the default), on a3_page.html, with the chrome booted and stacked."""
        if not self.open_page(c, "a3_page.html", env=env):
            return False
        self.app.wait_log(r'RustKit chrome IPC body=\{"cmd":"get_workspaces"', 20)
        self.expect(c, "the chrome was stacked over the page",
                    bool(self.app.wait_log(r"RustKit chrome stacked over the page", 5)))
        self.fixture.wait("beacon?a3-ready-", 10, self.since)
        time.sleep(1.5)
        return True

    def capture(self, c, name):
        """The window as a PIL image and its scale (pixels per point), or None."""
        if not self.can_capture:
            return None
        path = self.out / ("%s.%s.png" % (c, name))
        code, win = self.hw("window", self.app.proc.pid)
        subprocess.run(["/usr/sbin/screencapture", "-x", "-o", "-l", str(win.get("id", 0)), str(path)],
                       capture_output=True)
        if not path.is_file():
            return None
        from PIL import Image
        img = Image.open(path).convert("RGB")
        return img, img.width / float(win["w"]), win

    @staticmethod
    def is_green(rgb, tol=48):
        return all(abs(rgb[i] - GREEN[i]) <= tol for i in range(3))

    def green_at(self, shot, x, y):
        img, scale, _ = shot
        return self.is_green(img.getpixel((int(x * scale), int(y * scale))))

    def page_clicks(self, mark):
        return [p for p in self.fixture.seen("beacon?a3-click-", mark)]

    def a3_passthrough_click(self):
        """CH3.A3: a click on the page under the chrome's transparent area reaches the page, not the chrome."""
        c = "a3_passthrough_click"
        if not self.a3_open(c):
            return
        shot = self.capture(c, "page")
        if shot is None:
            self.not_run(c, "the page shows through the chrome view", self.why_not("Screen Recording"))
        else:
            img, scale, win = shot
            # The chrome view is the whole window and on top: the page's green
            # can only be seen where the chrome is absent.
            self.expect(c, "the page's green shows through the chrome view (window centre)",
                        self.green_at(shot, win["w"] / 2, win["h"] * 0.6))
            self.expect(c, "the toolbar is still the chrome's (not green)",
                        not self.green_at(shot, win["w"] / 2, self.TITLE_BAR + 50))
        if not self.can_input:
            return self.not_run(c, "a click on the page reaches it", self.why_not("Accessibility"))
        code, win = self.hw("window", self.app.proc.pid)
        mark, log_from = self.fixture.mark(), len(self.app.log_text())
        self.hw("click", self.app.proc.pid, int(win["w"] * 0.5), int(win["h"] * 0.6))
        self.expect(c, "the page's button heard the click (its beacon)",
                    self.fixture.wait("beacon?a3-click-", 5, mark), str(self.page_clicks(mark)))
        time.sleep(0.3)
        text = self.app.log_text()[log_from:]
        self.expect(c, "the app logged a content click", "content click (view-local)" in text)
        self.expect(c, "the chrome did not take the click", "chrome click (view-local)" not in text)
        # And the chrome over its own area still takes the pointer.
        log_from = len(self.app.log_text())
        self.hw("click", self.app.proc.pid, int(win["w"] * 0.55), 100)
        self.expect(c, "a click on the toolbar still reaches the chrome",
                    bool(self.app.wait_log(r"chrome click \(view-local\)", 3, log_from)))

    A3_MENU_JS = ("function a3menu() { showPageContextMenu({ target: document.getElementById('urlInput'), "
                  "preventDefault: function () {}, clientX: 600, clientY: 70 }); } "
                  "setTimeout(a3menu, 6000); setTimeout(a3menu, 14000);")
    OVERLAY = (r"RustKit chrome overlays count=([1-9]\d*) rects=\[RegionRect \{ x: ([\d.]+), y: ([\d.]+), "
               r"width: ([\d.]+), height: ([\d.]+)")

    def a3_dropdown_over_page(self):
        """CH3.A3: a chrome popup (the page context menu, opened at the toolbar) paints over the page, takes clicks there, and a page click outside closes it."""
        c = "a3_dropdown_over_page"
        # The downloads and history panels are placed below the viewport by
        # their own CSS (top: calc(100% + 6px) of the initial containing
        # block), so the check opens the chrome's context menu, as a right
        # click in the toolbar would, 6 s after boot and again at 14 s.
        ran_at = None
        if not self.a3_open(c, {"HIWAVE_RUSTKIT_CHROME_EVAL": self.A3_MENU_JS}):
            return
        log_from = len(self.app.log_text())
        before = self.capture(c, "before")
        m = self.app.wait_log(self.OVERLAY, 12, log_from)
        self.expect(c, "the chrome reported the open menu's box", bool(m), m.group(0) if m else "no overlay report")
        if not m:
            return
        x, y, w, h = (float(m.group(i)) for i in range(2, 6))
        self.expect(c, "the menu reaches below the toolbar, over the page", y + h > 140, "box %s" % [x, y, w, h])
        time.sleep(1.0)
        after = self.capture(c, "after")
        if before is None or after is None:
            self.not_run(c, "the menu paints over the page", self.why_not("Screen Recording"))
        else:
            px, py = x + w / 2, self.TITLE_BAR + (y + h + 104) / 2 + 10
            self.expect(c, "before: the page is where the menu opens", self.green_at(before, px, py),
                        "at %d,%d" % (px, py))
            self.expect(c, "after: the menu paints over the page there", not self.green_at(after, px, py),
                        "at %d,%d" % (px, py))
            self.expect(c, "after: the page still shows beside and below the menu",
                        self.green_at(after, x + w + 40, py) and self.green_at(after, px, self.TITLE_BAR + y + h + 30))
            # The page's pixels: its green, or (CH3.A3c, the transparent
            # chrome) that green under the menu's shadow, which now reaches
            # the page outside the menu's box.
            def page_px(p):
                return self.is_green(p) or (p[1] > 60 and p[1] > p[0] + 40 and p[1] > p[2] + 40)
            g0 = sum(n for (n, p) in before[0].getcolors(maxcolors=1 << 24) if page_px(p))
            g1 = sum(n for (n, p) in after[0].getcolors(maxcolors=1 << 24) if page_px(p))
            scale = after[1]
            self.expect(c, "the page did not move: it lost about the menu's area only",
                        0 < g0 - g1 < w * h * scale * scale * 1.2,
                        "green px before %d after %d, menu %d px" % (g0, g1, w * h * scale * scale))
        if not self.can_input:
            return self.not_run(c, "clicks on and beside the menu", self.why_not("Accessibility"))
        pid = self.app.proc.pid
        # A click on the menu's last row, which lies over the page, is the chrome's.
        log_from, mark = len(self.app.log_text()), self.fixture.mark()
        self.hw("click", pid, int(x + w / 2), int(self.TITLE_BAR + y + h - 18))
        hit = self.app.wait_log(r"chrome click \(view-local\)", 3, log_from)
        self.expect(c, "a click on the menu over the page reaches the chrome", bool(hit))
        time.sleep(0.5)
        text = self.app.log_text()[log_from:]
        self.expect(c, "the page did not get that click",
                    "content click (view-local)" not in text and not self.fixture.seen("beacon?a3-click-", mark))
        self.expect(c, "the menu closed after its item was clicked",
                    bool(self.app.wait_log(r"RustKit chrome overlays count=0", 3, log_from)))
        # Reopened at 14 s: a click on the page outside it reaches the page and closes it.
        log_from = len(self.app.log_text())
        m2 = self.app.wait_log(self.OVERLAY, 12, log_from)
        if not m2:
            return self.expect(c, "the menu opened again", False)
        log_from, mark = len(self.app.log_text()), self.fixture.mark()
        code, win = self.hw("window", pid)
        self.hw("click", pid, int(win["w"] * 0.5), int(win["h"] * 0.75))
        self.expect(c, "with the menu open, a click on the page beside it reaches the page",
                    self.fixture.wait("beacon?a3-click-", 5, mark))
        self.expect(c, "and closes the menu",
                    bool(self.app.wait_log(r"RustKit chrome overlays count=0", 3, log_from)))

    def a3b_context_menu_over_page(self):
        """CH3.A3b: a right click in the tab strip off any tab opens the chrome's own context menu, which paints over the page and takes a click on its item there."""
        c = "a3b_context_menu_over_page"
        if not self.a3_open(c):
            return
        if not self.can_input:
            return self.not_run(c, "a right click in the tab strip", self.why_not("Accessibility"))
        pid = self.app.proc.pid
        code, win = self.hw("window", pid)
        # The tab strip right of the tabs: no tab under the pointer, so the
        # chrome shows its page menu (showPageContextMenu), not the tab panel.
        at = (int(win["w"] * 0.8), self.TITLE_BAR + 22)
        before = self.capture(c, "before")
        log_from = len(self.app.log_text())
        self.hw("rightclick", pid, *at)
        m = self.app.wait_log(self.OVERLAY, 5, log_from)
        self.expect(c, "the menu's paint over the page is in the chrome's region", bool(m),
                    m.group(0) if m else "no overlay")
        if not m:
            return
        time.sleep(0.8)  # the menu's opening transition
        last = None
        for last in re.finditer(self.OVERLAY, self.app.log_text()[log_from:]):
            pass
        x, y, w, h = (float(last.group(i)) for i in range(2, 6))
        self.expect(c, "the menu opened at the pointer and reaches over the page",
                    abs(x - at[0]) <= 2 and y + h > 150, "box %s, pointer %s" % ([x, y, w, h], at))
        after = self.capture(c, "after")
        if before is None or after is None:
            self.not_run(c, "the menu paints over the page", self.why_not("Screen Recording"))
        else:
            px, py = x + w / 2, self.TITLE_BAR + y + h - 24
            self.expect(c, "before: the page is where the menu opens", self.green_at(before, px, py),
                        "at %d,%d" % (px, py))
            self.expect(c, "after: the menu paints over the page there (not clipped at the strip)",
                        not self.green_at(after, px, py), "at %d,%d" % (px, py))
            self.expect(c, "after: the page still shows beside the menu",
                        self.green_at(after, x - 40, py))
        # Its last item lies over the page: the click is the chrome's, the
        # item runs (an IPC from the chrome) and the menu closes.
        log_from, mark = len(self.app.log_text()), self.fixture.mark()
        self.hw("click", pid, int(x + w / 2), int(self.TITLE_BAR + y + h - 22))
        self.expect(c, "a click on the menu's item over the page reaches the chrome",
                    bool(self.app.wait_log(r"chrome click \(view-local\)", 3, log_from)))
        self.expect(c, "the item ran (the chrome posted its command)",
                    bool(self.app.wait_log(r"RustKit chrome IPC body=\{\"cmd\"", 3, log_from)))
        self.expect(c, "the menu closed and left the region",
                    bool(self.app.wait_log(r"RustKit chrome overlays count=0", 3, log_from)))
        time.sleep(0.3)
        self.expect(c, "the page did not get that click",
                    "content click (view-local)" not in self.app.log_text()[log_from:]
                    and not self.fixture.seen("beacon?a3-click-", mark))

    # A box no popup selector names, over the page column, opened 6 s after
    # boot: what a new chrome popup or tooltip looks like to the region.
    A3B_PROBE_JS = ("setTimeout(function () { var d = document.createElement('div'); d.id = 'a3bProbe'; "
                    "d.style.cssText = 'position:absolute;z-index:9999;left:640px;top:260px;width:160px;"
                    "height:90px;background:#0000ff'; d.addEventListener('click', function () { "
                    "d.remove(); sendIpc('a3b_probe_clicked', {}); }); document.body.appendChild(d); }, 6000);")

    def a3b_unlisted_popup_over_page(self):
        """CH3.A3b: a chrome box no selector names (a new popup or tooltip) paints over the page and takes clicks there."""
        c = "a3b_unlisted_popup_over_page"
        if not self.a3_open(c, {"HIWAVE_RUSTKIT_CHROME_EVAL": self.A3B_PROBE_JS}):
            return
        # Opened 6 s after the chrome booted; a3_open waits about 3 s of that.
        time.sleep(5.0)
        shot = self.capture(c, "open")
        if shot is None:
            self.not_run(c, "the box paints over the page", self.why_not("Screen Recording"))
        else:
            img, scale, win = shot
            px, py = 720, self.TITLE_BAR + 305
            rgb = img.getpixel((int(px * scale), int(py * scale)))
            self.expect(c, "the box's blue shows over the page", all(abs(rgb[i] - BLUE[i]) <= 48 for i in range(3)),
                        "at %d,%d: %s" % (px, py, rgb))
            self.expect(c, "the page shows beside it", self.green_at(shot, 600, py))
        if not self.can_input:
            return self.not_run(c, "a click on the box", self.why_not("Accessibility"))
        pid = self.app.proc.pid
        log_from, mark = len(self.app.log_text()), self.fixture.mark()
        self.hw("click", pid, 720, self.TITLE_BAR + 305)
        self.expect(c, "a click on the box over the page reaches the chrome's handler",
                    bool(self.app.wait_log(r'RustKit chrome IPC body=\{"cmd":"a3b_probe_clicked"', 3, log_from)))
        time.sleep(0.3)
        self.expect(c, "the page did not get that click",
                    "content click (view-local)" not in self.app.log_text()[log_from:]
                    and not self.fixture.seen("beacon?a3-click-", mark))
        self.expect(c, "the removed box left the region",
                    bool(self.app.wait_log(r"RustKit chrome overlays count=0", 3, log_from)))

    def a3_cmd_l_from_page(self):
        """CH3.A3: with the keyboard on the page, Cmd+L focuses the chrome's address bar and typing goes there."""
        c = "a3_cmd_l_from_page"
        if not self.a3_open(c):
            return
        if not self.can_input:
            return self.not_run(c, "Cmd+L from the page", self.why_not("Accessibility"))
        pid = self.app.proc.pid
        code, win = self.hw("window", pid)
        mark = self.fixture.mark()
        self.hw("click", pid, int(win["w"] * 0.5), int(win["h"] * 0.6))
        self.expect(c, "the page took the click", self.fixture.wait("beacon?a3-click-", 5, mark))
        time.sleep(0.3)
        mark = self.fixture.mark()
        self.hw("type", pid, "k")
        self.expect(c, "the page has the keyboard (its keydown heard k)",
                    self.fixture.wait("beacon?a3-key-k", 5, mark))
        log_from = len(self.app.log_text())
        self.hw("key", pid, 37, "cmd")  # Cmd+L
        time.sleep(0.6)
        mark = self.fixture.mark()
        # The bar holds the page's URL and Cmd+L does not select it yet
        # (CH2.E12), so the typed text is appended: look for it in the URL.
        self.hw("type", pid, "zzqa3")
        self.hw("key", pid, 36)
        ipc = self.app.wait_log(r'RustKit chrome IPC body=\{"cmd":"navigate","url":"([^"]*)"', 8, log_from)
        self.expect(c, "Return in the address bar posted navigate with the typed text",
                    bool(ipc) and "zzqa3" in ipc.group(1), ipc.group(1) if ipc else "no navigate")
        self.expect(c, "the page did not hear the typed text", not self.fixture.seen("beacon?a3-key-z", mark))

    def a3_resize(self):
        """CH3.A3: after a window resize the chrome's hole follows the page: page visible to the corner, clicks pass through."""
        c = "a3_resize"
        if not self.a3_open(c):
            return
        if not self.can_resize:
            return self.not_run(c, "resize", self.why_not("Accessibility"))
        pid = self.app.proc.pid
        for (w, h) in [(900, 620), (1500, 900)]:
            mark = self.fixture.mark()
            code, out = self.hw("resize", pid, w, h)
            self.expect(c, "the window took the size %dx%d" % (w, h), code == 0, json.dumps(out)[:200])
            self.expect(c, "the page heard resize at %dx%d" % (w, h),
                        self.fixture.wait("beacon?a3-resize-", 5, mark),
                        str(self.fixture.seen("beacon?a3-resize-", mark)[-1:]))
            time.sleep(1.2)
            code, win = self.hw("window", pid)
            shot = self.capture(c, "at-%dx%d" % (w, h))
            if shot is None:
                self.not_run(c, "frames at %dx%d" % (w, h), self.why_not("Screen Recording"))
            else:
                self.expect(c, "the page shows to the bottom-right corner at %dx%d" % (w, h),
                            self.green_at(shot, win["w"] - 12, win["h"] - 12))
                self.expect(c, "the toolbar is the chrome's at %dx%d" % (w, h),
                            not self.green_at(shot, win["w"] * 0.5, self.TITLE_BAR + 50))
            if self.can_input:
                mark, log_from = self.fixture.mark(), len(self.app.log_text())
                self.hw("click", pid, int(win["w"] - 40), int(win["h"] - 40))
                self.expect(c, "a click near the new bottom-right corner reaches the page at %dx%d" % (w, h),
                            self.fixture.wait("beacon?a3-click-", 5, mark))
                self.hw("click", pid, int(win["w"] * 0.55), 100)
                self.expect(c, "a toolbar click reaches the chrome at %dx%d" % (w, h),
                            bool(self.app.wait_log(r"chrome click \(view-local\)", 3, log_from)))

    def e12_input_select_all(self):
        """CH2.E12: click a filled <input>, Cmd+A, type: the typed text replaces the value; a focus handler's select() survives the click."""
        c = "e12_input_select_all"
        if not self.open_page(c, "e12_input_select.html"):
            return
        if not self.can_input:
            return self.not_run(c, "click, Cmd+A and type replace the value", self.why_not("Accessibility"))
        self.fixture.wait("beacon?e12-ready", 10, self.since)
        pid = self.app.proc.pid
        code, win = self.hw("window", pid)
        w, h = win["w"], win["h"]

        def last(prefix, mark):
            seen = [urllib.parse.unquote(p) for p in self.fixture.seen("beacon?" + prefix, mark)]
            return seen[-1].split("?", 1)[1] if seen else None

        # #a: a click right of its text (the caret goes to the end), then
        # Cmd+A (keycode 0 = A), then "new". Chrome 143: "new".
        mark = self.fixture.mark()
        self.hw("click", pid, w * 0.7, h * 0.35)
        self.expect(c, "#a took the focus", self.fixture.wait("beacon?e12-a-focus", 4, mark))
        time.sleep(0.4)
        self.hw("key", pid, 0, "cmd")
        time.sleep(0.4)
        self.fixture.wait("beacon?e12-a-v=", 3, mark)
        after_a = last("e12-a-v=", mark)
        self.expect(c, "Cmd+A selects the whole value (Chrome: 0,9)",
                    after_a == "e12-a-v=old-value-s=0,9", str(after_a))
        self.hw("type", pid, "new")
        time.sleep(0.8)
        typed = last("e12-a-v=", mark)
        self.expect(c, "typing after Cmd+A replaces the value (Chrome: new)",
                    typed is not None and typed.startswith("e12-a-v=new-"), str(typed))
        # #b: its focus handler calls select(); a click focuses it and the
        # selection stands, so typing replaces. Chrome 143: "new".
        mark = self.fixture.mark()
        self.hw("click", pid, w * 0.3, h * 0.75)
        self.expect(c, "#b took the focus", self.fixture.wait("beacon?e12-b-focus", 4, mark))
        time.sleep(0.4)
        self.hw("type", pid, "new")
        time.sleep(0.8)
        typed = last("e12-b-v=", mark)
        self.expect(c, "a click into a field whose focus handler select()s, then typing, replaces (Chrome: new)",
                    typed is not None and typed.startswith("e12-b-v=new-"), str(typed))

    # ── CH2.E4: the mouse surface ─────────────────────────────────────────

    E4_LOG = {"RUST_LOG": "info,rustkit_viewhost=debug"}

    def e4_open(self, c, query, env=None):
        """e4_mouse.html with `query`, the input grant, and the page's ready beacon."""
        env = dict(self.E4_LOG, **(env or {}))
        if not self.open_page(c, "e4_mouse.html?" + query, env=env):
            return None
        self.expect(c, "the page's script ran", self.fixture.wait("beacon?e4-ready", 10, self.since))
        if not self.can_input:
            self.not_run(c, "real input", self.why_not("Accessibility"))
            return None
        time.sleep(0.5)
        return self.centre()

    def menus(self):
        code, out = self.hw("menus", self.app.proc.pid)
        return out.get("count", 0) if code == 0 else -1

    def e4_contextmenu(self):
        """CH2.E4: a right press reaches the page as contextmenu (button 2) and, not cancelled, opens the native menu."""
        c = "e4_contextmenu"
        at = self.e4_open(c, "words")
        if at is None:
            return
        pid, log_from, mark = self.app.proc.pid, len(self.app.log_text()), self.fixture.mark()
        before = self.menus()
        self.hw("rightpress", pid, at[0], at[1])
        self.expect(c, "the page heard contextmenu with button 2, not cancelled",
                    self.fixture.wait("beacon?e4-contextmenu-button-2-prevented-0", 5, mark))
        shown = self.app.wait_log(r"page context menu shown", 5, log_from)
        self.expect(c, "the app showed its context menu", bool(shown))
        time.sleep(0.8)
        after = self.menus()
        self.expect(c, "a menu window is on screen", after > before, "menu windows %d -> %d" % (before, after))
        self.hw("key", pid, 53)
        time.sleep(0.8)
        self.expect(c, "Escape closed it", self.menus() == before, "menu windows now %d" % self.menus())
        self.hw("release", pid, at[0], at[1])
        self.expect(c, "the app is still running", self.app.alive())

    def e4_contextmenu_prevented(self):
        """CH2.E4: a contextmenu the page cancels opens no menu, and the release reaches the page as auxclick."""
        c = "e4_contextmenu_prevented"
        at = self.e4_open(c, "words&prevent")
        if at is None:
            return
        pid, log_from, mark = self.app.proc.pid, len(self.app.log_text()), self.fixture.mark()
        before = self.menus()
        self.hw("rightclick", pid, at[0], at[1])
        self.expect(c, "the page heard contextmenu with button 2 and cancelled it",
                    self.fixture.wait("beacon?e4-contextmenu-button-2-prevented-1", 5, mark))
        self.expect(c, "the page heard auxclick with button 2",
                    self.fixture.wait("beacon?e4-auxclick-button-2", 5, mark))
        time.sleep(0.8)
        self.expect(c, "no menu was shown", self.menus() == before and
                    not re.search(r"page context menu shown", self.app.log_text()[log_from:]),
                    "menu windows %d" % self.menus())

    def e4_dblclick_selects_word(self):
        """CH2.E4: a double click fires dblclick with detail 2 and selects the word under it."""
        c = "e4_dblclick_selects_word"
        at = self.e4_open(c, "words")
        if at is None:
            return
        mark = self.fixture.mark()
        self.hw("dblclick", self.app.proc.pid, at[0], at[1])
        self.expect(c, "the second click had detail 2", self.fixture.wait("beacon?e4-click-detail-2", 5, mark))
        got = self.fixture.wait("beacon?e4-dblclick-detail-2-selection-bravo", 5, mark)
        self.expect(c, "dblclick (detail 2) found 'bravo' selected", got,
                    str(self.fixture.seen("beacon?e4-dblclick", mark)))

    def e4_drag(self):
        """CH2.E4: a move with the button held reaches the page with buttons 1."""
        c = "e4_drag"
        at = self.e4_open(c, "words")
        if at is None:
            return
        mark = self.fixture.mark()
        self.hw("drag", self.app.proc.pid, at[0] - 120, at[1], at[0] + 120, at[1])
        self.expect(c, "pointermove with buttons 1 during the drag",
                    self.fixture.wait("beacon?e4-dragmove-buttons-1", 5, mark))

    def cursor_now(self):
        code, out = self.hw("cursor")
        return out.get("cursor", "?") if code == 0 else "?"

    def e4_cursor_hand(self):
        """CH2.E4: the CSS cursor under the pointer is the window's: a link's hand, ew-resize's arrows, the arrow again outside."""
        c = "e4_cursor_hand"
        at = self.e4_open(c, "")
        if at is None:
            return
        pid, log_from = self.app.proc.pid, len(self.app.log_text())
        self.hw("move", pid, at[0], at[1] - 100)
        set_hand = self.app.wait_log(r'content view cursor set css="?pointer', 5, log_from)
        self.expect(c, "the app set the link's cursor", bool(set_hand))
        time.sleep(0.4)
        now = self.cursor_now()
        self.expect(c, "over the link the cursor is the pointing hand", now == "pointingHand", now)
        self.hw("move", pid, at[0], at[1] + 150)
        time.sleep(0.6)
        now = self.cursor_now()
        self.expect(c, "over cursor:ew-resize it is the left-right resize cursor", now == "resizeLeftRight", now)
        self.hw("move", pid, at[0], 12)
        time.sleep(0.6)
        now = self.cursor_now()
        self.expect(c, "in the title bar it is the arrow again", now == "arrow", now)

    def e4_tooltip(self):
        """CH2.E4: the `title` under the pointer becomes the content view's tooltip and AppKit shows it."""
        c = "e4_tooltip"
        at = self.e4_open(c, "")
        if at is None:
            return
        pid, log_from = self.app.proc.pid, len(self.app.log_text())
        before = self.menus()
        self.hw("move", pid, at[0] - 3, at[1] - 100)
        self.hw("move", pid, at[0], at[1] - 100)
        tip = self.app.wait_log(r'content view tooltip set tooltip=Some\("the E4 tooltip"\)', 5, log_from)
        self.expect(c, "the app set the link's title as the tooltip", bool(tip))
        time.sleep(3.0)
        after = self.menus()
        self.expect(c, "a tooltip window is on screen after a rest", after > before, "windows %d -> %d" % (before, after))
        self.hw("move", pid, at[0], at[1] + 150)
        cleared = self.app.wait_log(r"content view tooltip set tooltip=None", 5, log_from)
        self.expect(c, "off the link the tooltip is cleared", bool(cleared))

    # The RustKit chrome's first tab: the tab strip right of the 220 px
    # sidebar, under the 32 px title bar (see a2_chrome_typed_url).
    E4_TAB = (440, 56)

    def e4_chrome_tab(self):
        """CH2.E4, RustKit chrome (the default): over a tab of the RustKit chrome the cursor is the hand; a right press there reaches the chrome's contextmenu handler (no native menu)."""
        c = "e4_chrome_tab"
        env = dict(self.E4_LOG)
        if not self.open_page(c, "e4_mouse.html", env=env):
            return
        self.app.wait_log(r'RustKit chrome IPC body=\{"cmd":"get_workspaces"', 20)
        time.sleep(1.5)
        if not self.can_input:
            return self.not_run(c, "hover and right press on a tab", self.why_not("Accessibility"))
        pid, log_from = self.app.proc.pid, len(self.app.log_text())
        self.hw("move", pid, self.E4_TAB[0] - 4, self.E4_TAB[1])
        self.hw("move", pid, *self.E4_TAB)
        time.sleep(0.6)
        now = self.cursor_now()
        self.expect(c, "over a tab the cursor is the pointing hand", now == "pointingHand", now)
        before = self.menus()
        self.hw("rightclick", pid, *self.E4_TAB)
        time.sleep(1.0)
        self.expect(c, "the chrome took the right press (no native menu, app alive)",
                    self.app.alive() and self.menus() == before and
                    not re.search(r"page context menu shown", self.app.log_text()[log_from:]),
                    "menu windows %d -> %d" % (before, self.menus()))
        if self.can_capture:
            path = self.out / ("%s.after.png" % c)
            code, win = self.hw("window", pid)
            subprocess.run(["/usr/sbin/screencapture", "-x", "-o", "-l", str(win.get("id", 0)), str(path)])

    # ── M1: Cmd+W closes the active tab every time ─────────────────────────

    CLOSE_IPC = r'RustKit chrome IPC body=\{"cmd":"close_tab","id":"(\d+)"\}'

    def m1_tabs(self, c, n):
        """Cmd+T until `n` more tabs exist. Returns the new tab ids, in order."""
        pid, made = self.app.proc.pid, []
        for _ in range(n):
            log_from = len(self.app.log_text())
            self.hw("key", pid, 17, "cmd")
            m = self.app.wait_log(r"Created tab: TabId\((\d+)\)", 5, log_from)
            if m:
                made.append(m.group(1))
            time.sleep(0.8)
        self.expect(c, "Cmd+T made %d tabs" % n, len(made) == n, "made %s" % made)
        return made

    def m1_cmd_w(self, c, what, want=None):
        """One Cmd+W: the chrome posts close_tab (for tab `want` if given) and the app closes it.
        Returns the id it closed, or None."""
        pid, log_from = self.app.proc.pid, len(self.app.log_text())
        self.hw("key", pid, 13, "cmd")
        ipc = self.app.wait_log(self.CLOSE_IPC, 3, log_from)
        closed = self.app.wait_log(r"Closed tab: TabId\((\d+)\)", 3, log_from) if ipc else None
        ok = bool(ipc) and bool(closed) and (want is None or ipc.group(1) == want)
        self.expect(c, "%s: the chrome sent close_tab%s and the app closed it" % (
            what, " for tab %s" % want if want else ""), ok,
            ("close_tab %s, closed %s" % (ipc.group(1), closed.group(1) if closed else "nothing")) if ipc
            else ("menu event logged, no close_tab in 3 s" if self.app.wait_log(r'menu event id="close_tab"', 0, log_from)
                  else "no menu event in 3 s"))
        time.sleep(0.8)
        return ipc.group(1) if ok else None

    def m1_saved_tabs(self):
        """How many tabs the app's saved state holds (it saves on each close)."""
        try:
            return len(tab_urls(json.loads(self.app.state_file.read_text())))
        except (OSError, ValueError):
            return None

    def m1_cmd_w_repeat(self):
        """M1: three tabs, Cmd+W three times: each press closes the active tab and no tab is left."""
        c = "m1_cmd_w_repeat"
        if not self.open_page(c, "e9_final.html"):
            return
        self.app.wait_log(r'RustKit chrome IPC body=\{"cmd":"get_workspaces"', 20)
        time.sleep(1.0)
        if not self.can_input:
            return self.not_run(c, "Cmd+W three times", self.why_not("Accessibility"))
        restored = self.m1_saved_tabs()
        self.expect(c, "the app was restored on one tab", restored == 1, "saved tabs %s" % restored)
        self.m1_tabs(c, 2)
        for press in (1, 2, 3):
            self.m1_cmd_w(c, "Cmd+W #%d" % press)
        left = self.m1_saved_tabs()
        self.expect(c, "no tab is left after three Cmd+W", left == 0, "saved tabs %s" % left)
        self.expect(c, "the app is still running", self.app.alive())

    def m1_cmd_w_after_clicks(self):
        """M1: Cmd+W after clicking a background tab closes that tab; after a tab's close button, Cmd+W closes the active tab of the rest."""
        c = "m1_cmd_w_after_clicks"
        if not self.open_page(c, "e9_final.html"):
            return
        self.app.wait_log(r'RustKit chrome IPC body=\{"cmd":"get_workspaces"', 20)
        time.sleep(1.0)
        if not self.can_input:
            return self.not_run(c, "clicks on tabs, then Cmd+W", self.why_not("Accessibility"))
        pid = self.app.proc.pid
        made = self.m1_tabs(c, 3)
        # Cmd+T left the last new tab active. Walk the strip's row left to
        # right (6 px steps; a tab is 100-180 px wide) until a click
        # activates another tab: where a tab sits is not fixed, the strip
        # scrolls its active tab to the centre and clips the tab at its left
        # edge. A tab's close button shows under the pointer, so the walk
        # can close a tab (a clipped tab's button can be the first thing it
        # meets): then Cmd+T makes a known active tab again and the walk
        # starts over, at most three times.
        act, got, active = None, None, made[-1] if made else None
        for _ in range(3):
            for x in range(232, 1000, 6):
                at = len(self.app.log_text())
                self.hw("click", pid, x, self.E4_TAB[1] - 9)
                got = self.app.wait_log(r'RustKit chrome IPC body=\{"cmd":"(activate_tab|close_tab)","id":"(\d+)"\}', 0.4, at)
                if got and (got.group(1) == "close_tab" or got.group(2) != active):
                    break
            if got and got.group(1) == "activate_tab":
                act = got
                break
            time.sleep(0.8)
            again = self.m1_tabs(c, 1)
            active = again[-1] if again else active
        self.expect(c, "a click on a background tab activates it", bool(act),
                    act.group(0) if act else ("last: %s" % got.group(0) if got else "no other tab on the row"))
        time.sleep(0.8)
        if act:
            self.m1_cmd_w(c, "Cmd+W after the click", act.group(2))
        # A tab's close button shows on the active tab and under the
        # pointer. Walk the strip's row left to right (4 px steps, the
        # button is 16 px) until a click closes a tab. Each click on a tab
        # activates it and the strip scrolls it to the centre, so the row
        # moves under the walk: up to three walks.
        # The tab's block button (block the site and close the tab) sits
        # left of it and shows the same way: a walk that meets it first
        # stops there and the step is NOT RUN, not a fail.
        hit = blocked = None
        for _ in range(3):
            for x in range(232, 1000, 4):
                at = len(self.app.log_text())
                self.hw("click", pid, x, self.E4_TAB[1])
                hit = self.app.wait_log(self.CLOSE_IPC, 0.4, at)
                blocked = self.app.wait_log(r'"cmd":"block_and_close"', 0, at)
                if hit or blocked:
                    break
            if hit or blocked:
                break
        if blocked and not hit:
            return self.not_run(c, "a click on a tab's close button closes it, then Cmd+W",
                                "the walk met the tab's block button first (x=%d)" % x)
        self.expect(c, "a click on a tab's close button closes it", bool(hit),
                    hit.group(0) if hit else "no close_tab while walking the strip")
        time.sleep(0.8)
        if hit:
            self.m1_cmd_w(c, "Cmd+W after the close button")
        self.expect(c, "the app is still running", self.app.alive())

    # ── CH3.A4: the shelf, the Inspector and settings as RustKit views ──────

    # The panel probe reports shapes only (#910 R1): per selector the first
    # match's box, the match count `n`, a checkbox's `checked` and a field's
    # value length `len`. No text, no values. It is compiled into debug
    # builds and `--features test-hooks` release builds only.
    A4_PROBE = ",".join([
        "#commandInput", ".shelf-title", ".command-item", ".command-item[data-id=new_tab]",
        ".command-item[data-id=new_workspace]",
        "[data-tab=console]", "#console-input", ".dom-node", ".console-entry", ".console-entry.result",
        ".console-entry.error",
        "#sidebarDefaultOpen", "#saveBtn", ".section", "#focusBlocklistInput", "#focusBlocklistAddBtn",
        ".blocklist-item", ".blocklist-item[data-domain=\"a4-blocked.example\"]", ".status.show.success",
        "#vaultPassword", "#newCredUrl", "#newCredUsername", "#newCredPassword", "#addCredentialBtn",
    ])

    def a4_open(self, c, env=None):
        """A fresh app, RustKit chrome (the default), panels probed, on a3_page.html (green, beacons on click/key)."""
        env = dict({"HIWAVE_RUSTKIT_PANEL_PROBE": self.A4_PROBE}, **(env or {}))
        if not self.a3_open(c, env):
            return False
        if self.app.wait_log(r"HIWAVE_RUSTKIT_PANEL_PROBE ignored", 1):
            self.not_run(c, "the panel checks", "the binary has no panel probe: build it with "
                         "`--features test-hooks` (or a debug build)")
            return False
        return True

    def probes(self, panel, start=0):
        """Every probe report a RustKit panel posted since log offset `start`, oldest first."""
        out = []
        pat = r'RustKit panel probe panel="%s" body=(\{[^\n]*\})' % re.escape(panel)
        for m in re.finditer(pat, self.app.log_text()[start:]):
            try:
                out.append(json.loads(m.group(1)))
            except ValueError:
                pass
        return out

    def wait_probe(self, panel, pred, timeout, start=0):
        """The first probe report of `panel` since `start` for which `pred` holds, or None."""
        end = time.time() + timeout
        while True:
            for p in self.probes(panel, start):
                if pred(p):
                    return p
            if time.time() >= end or not self.app.alive():
                return None
            time.sleep(0.2)

    def last_probe(self, panel, start=0):
        got = self.probes(panel, start)
        return got[-1] if got else None

    def panel_ipc(self, panel, cmd, timeout, start=0):
        """The app took `cmd` from the RustKit `panel` (the log names the cmd and size only)."""
        return self.app.wait_log(r'RustKit panel IPC panel="%s" cmd=%s len=\d+' % (re.escape(panel), cmd),
                                 timeout, start)

    @staticmethod
    def el(p, sel):
        """A probe's shape for `sel`: {box, n, checked, len}, or {} when nothing matches."""
        return ((p or {}).get("boxes", {}) or {}).get(sel) or {}

    @classmethod
    def count(cls, p, sel):
        return cls.el(p, sel).get("n") or 0

    def flat_band(self, shot, x0, y0, x1, y1):
        """How many colours the window shows in a box given in points (1 = one flat colour)."""
        img, scale, _ = shot
        band = img.crop((int(x0 * scale), int(y0 * scale), int(x1 * scale), int(y1 * scale)))
        return len(band.getcolors(maxcolors=band.width * band.height + 1) or [])

    def a4_shelf(self):
        """CH3.A4: RustKit chrome (the default), the shelf (command palette) is a RustKit view: opens, renders its commands, runs one typed command, closes."""
        c = "a4_shelf"
        # Nothing in the app sends expand_shelf (the chrome has its own
        # command palette on Cmd+K), so the check posts it from the chrome,
        # as a chrome button would.
        js = "setTimeout(function () { sendIpc('expand_shelf'); }, 4000);"
        log_from = 0
        if not self.a4_open(c, {"HIWAVE_RUSTKIT_CHROME_EVAL": js}):
            return
        # CH3.A4b: the shelf is built when it opens, not at launch.
        built = self.app.wait_log(r"the shelf is a RustKit view", 15)
        asked = re.search(r'RustKit chrome IPC body=\{"cmd":"expand_shelf"', self.app.log_text())
        self.expect(c, "the shelf was built as a RustKit view when it opened (expand_shelf), not at launch",
                    bool(built and asked and built.start() > asked.start()),
                    "built %s, expand_shelf %s" % (built and built.start(), asked and asked.start()))
        q = self.panel_ipc("shelf", "search_commands", 15)
        self.expect(c, "the open shelf's script posted search_commands and the app took it", bool(q),
                    q.group(0)[:160] if q else "")
        rendered = self.wait_probe("shelf", lambda p: self.count(p, ".command-item[data-id=new_tab]") == 1
                                   and self.count(p, ".command-item") > 1
                                   and (self.el(p, ".shelf-title").get("box") or [0, 0, 0])[2] > 0, 8)
        self.expect(c, "the shelf rendered the commands the app sent back (hiwaveShelf.showCommands)", bool(rendered),
                    json.dumps((rendered or self.last_probe("shelf") or {}).get("boxes", "no probe"))[:240])
        if not rendered:
            return
        pw, ph = rendered["size"]
        self.expect(c, "the shelf is 280 pt tall", ph == 280, "size %s" % rendered["size"])
        time.sleep(0.8)
        code, win = self.hw("window", self.app.proc.pid)
        x0, y0 = win["w"] - pw, win["h"] - ph
        shot = self.capture(c, "open")
        if shot is None:
            self.not_run(c, "the shelf paints over the bottom of the page", self.why_not("Screen Recording"))
        else:
            self.expect(c, "the shelf paints where the page was green (bottom of the page column)",
                        not self.green_at(shot, x0 + pw / 2, y0 + 20) and not self.green_at(shot, x0 + 30, win["h"] - 20))
            colours = self.flat_band(shot, x0 + 4, y0 + 4, win["w"] - 4, win["h"] - 4)
            self.expect(c, "the shelf frame is not blank", colours > 30, "%d colours" % colours)
            self.expect(c, "the page still shows above the shelf", self.green_at(shot, x0 + pw / 2, y0 - 40))
        if not self.can_input:
            return self.not_run(c, "a typed command runs", self.why_not("Accessibility"))
        pid = self.app.proc.pid
        def workspaces():
            try:
                return [w.get("name") for w in json.loads(self.app.state_file.read_text())["shell"]["workspaces"]]
            except (FileNotFoundError, ValueError, KeyError):
                return []
        before = workspaces()
        log_from = len(self.app.log_text())
        self.hw("type", pid, "workspace")
        typed = self.wait_probe("shelf", lambda p: self.el(p, "#commandInput").get("len") == len("workspace"), 5,
                                log_from)
        self.expect(c, "typing goes to the shelf's input (its value is 9 long)", bool(typed),
                    str(self.el(self.last_probe("shelf", log_from), "#commandInput")))
        found = self.wait_probe("shelf", lambda p: self.count(p, ".command-item") == 1
                                and self.count(p, ".command-item[data-id=new_workspace]") == 1, 5, log_from)
        self.expect(c, "the shelf searched for the typed text and shows only the match (new_workspace)", bool(found),
                    json.dumps((self.last_probe("shelf", log_from) or {}).get("boxes", {}))[:240])
        self.hw("key", pid, 36)
        ex = self.panel_ipc("shelf", "execute_command", 5, log_from)
        self.expect(c, "Return posted execute_command", bool(ex), ex.group(0)[:120] if ex else "")
        after, end = before, time.time() + 5
        while time.time() < end and len(after) <= len(before):
            after = workspaces()
            time.sleep(0.2)
        self.expect(c, "the command ran: the saved state has a new workspace", len(after) == len(before) + 1,
                    "before %s, after %s" % (before, after))
        time.sleep(1.5)
        shot = self.capture(c, "closed")
        if shot is not None:
            code, win = self.hw("window", pid)
            self.expect(c, "closed: the page is green to the bottom", self.green_at(shot, x0 + pw / 2, win["h"] - 20))
        # The keyboard is the page's again.
        mark = self.fixture.mark()
        self.hw("type", pid, "q")
        self.expect(c, "after it closed, keys go to the page", self.fixture.wait("beacon?a3-key-q", 5, mark))

    def a4_inspector(self):
        """CH3.A4: RustKit chrome (the default), the Inspector is a RustKit view: Cmd+Opt+I opens it, it renders the page's tree, shows a page console line, evaluates a typed line, and closes."""
        c = "a4_inspector"
        if not self.a4_open(c):
            return
        if not self.can_input:
            return self.not_run(c, "Cmd+Opt+I", self.why_not("Accessibility"))
        pid = self.app.proc.pid
        log_from = opened_from = len(self.app.log_text())
        self.hw("key", pid, 34, "cmd", "alt")  # Cmd+Opt+I
        self.expect(c, "Cmd+Opt+I opened the Inspector",
                    bool(self.app.wait_log(r"Opening inspector panel", 5, log_from)))
        # CH3.A4b: the Inspector is built when it opens, not at launch.
        early = re.search(r"the Inspector is a RustKit view", self.app.log_text()[:log_from])
        self.expect(c, "the Inspector was built as a RustKit view when it opened, not at launch",
                    not early and bool(self.app.wait_log(r"the Inspector is a RustKit view", 5, log_from)))
        # The keyboard is the Inspector's on open (#910 R1 nit 6): a key
        # does not reach the page.
        mark = self.fixture.mark()
        self.hw("type", pid, "z")
        self.expect(c, "on open the keyboard is the Inspector's, not the page's",
                    not self.fixture.wait("beacon?a3-key-z", 1.5, mark))
        # a3_page.html is html, head (meta, title, style), body (button, script).
        tree = self.wait_probe("Inspector", lambda p: self.count(p, ".dom-node") >= 4 and p["size"][1] == 300, 10, log_from)
        self.expect(c, "the Inspector rendered the page's tree (4+ nodes)", bool(tree),
                    json.dumps((tree or self.last_probe("Inspector", log_from) or {}).get("boxes", "no probe"))[:240])
        if not tree:
            return
        pw, ph = tree["size"]
        time.sleep(0.8)
        code, win = self.hw("window", pid)
        x0, y0 = win["w"] - pw, win["h"] - ph
        shot = self.capture(c, "open")
        if shot is None:
            self.not_run(c, "the Inspector paints", self.why_not("Screen Recording"))
        else:
            self.expect(c, "the Inspector paints below the page (not green)",
                        not self.green_at(shot, x0 + pw / 2, y0 + ph / 2))
            colours = self.flat_band(shot, x0 + 4, y0 + 4, win["w"] - 4, win["h"] - 4)
            self.expect(c, "the Inspector frame is not blank", colours > 30, "%d colours" % colours)
            self.expect(c, "the page still shows above the Inspector", self.green_at(shot, x0 + pw / 2, y0 - 40))
        # A page click logs a console line, which the Inspector shows.
        log_from, mark = len(self.app.log_text()), self.fixture.mark()
        self.hw("click", pid, int(x0 + pw / 2), int(y0 - 60))
        self.expect(c, "the page heard the click", self.fixture.wait("beacon?a3-click-", 5, mark))
        # Its Console tab, then the line: the engine's Inspector shows a page
        # string as its length, "<string len=N>" (by design, T6a).
        tab = self.el(tree, "[data-tab=console]").get("box")
        if not tab:
            return self.expect(c, "the Console tab has a box", False, str(tree["boxes"]))
        log_from = len(self.app.log_text())
        self.hw("click", pid, int(x0 + tab[0] + tab[2] / 2), int(y0 + tab[1] + tab[3] / 2))
        console = self.wait_probe("Inspector", lambda p: (self.el(p, "#console-input").get("box") or [0, 0, 0, 0])[2] > 0,
                                  5, log_from)
        self.expect(c, "a click on the Console tab shows the console", bool(console))
        if not console:
            return
        line = self.wait_probe("Inspector", lambda p: self.count(p, ".console-entry") >= 1, 3, log_from)
        self.expect(c, "the Inspector shows the page's console line (one entry or more)", bool(line),
                    str(self.el(self.last_probe("Inspector", log_from), ".console-entry")))
        entries = self.count(line or console, ".console-entry")
        results = self.count(line or console, ".console-entry.result")
        # A line typed in its console is evaluated in the page: it fetches a
        # beacon named by its own value, and the Inspector shows the line
        # and a result.
        box = self.el(console, "#console-input")["box"]
        log_from = len(self.app.log_text())
        self.hw("click", pid, int(x0 + box[0] + min(box[2], 200) / 2), int(y0 + box[1] + box[3] / 2))
        clicked = self.app.wait_log(r"panel click \(view-local\)", 3, log_from)
        self.expect(c, "the click on the console input reached the Inspector", bool(clicked))
        time.sleep(0.4)
        mark = self.fixture.mark()
        self.hw("type", pid, "fetch('/beacon?a4-eval-' + 6*7)")
        self.hw("key", pid, 36)
        ev = self.panel_ipc("Inspector", "inspector_console_eval", 5, log_from)
        self.expect(c, "Return posted inspector_console_eval", bool(ev), ev.group(0)[:120] if ev else "")
        ran = self.fixture.wait("beacon?a4-eval-42", 6, mark)
        res = self.wait_probe("Inspector", lambda p: self.count(p, ".console-entry") >= entries + 2
                              and self.count(p, ".console-entry.result") > results, 6, log_from)
        self.expect(c, "the page ran the line (beacon a4-eval-42) and the Inspector shows the line and a result",
                    bool(ran) and bool(res),
                    "beacon %s, entries %s" % (bool(ran), json.dumps({k: self.el(self.last_probe("Inspector", log_from), k)
                                                                     for k in (".console-entry", ".console-entry.result",
                                                                               ".console-entry.error")})))
        # CH3.A4c: the Inspector's engine view is the one its page loaded
        # into when it opened; its forced layouts and relayouts are logged
        # under that id (shapes only: counts).
        vid = re.search(r"Loading HTML content id=EngineViewId\((\d+)\)", self.app.log_text()[opened_from:])
        vid = vid.group(1) if vid else None
        if vid is None:
            self.expect(c, "the log names the Inspector's view", False)
        else:
            base = self.count(self.last_probe("Inspector", log_from) or {}, ".console-entry")
            log_from = len(self.app.log_text())
            self.hw("type", pid, "for (let i = 0; i < 200; i++) console.log('a4c ' + i)")
            self.hw("key", pid, 36)
            shown = self.wait_probe("Inspector", lambda p: self.count(p, ".console-entry") >= base + 200, 15, log_from)
            time.sleep(1.0)
            forced = sum(int(n) for n in re.findall(
                r"Forced layouts in the script task id=EngineViewId\(%s\) forced_layouts=(\d+)" % vid,
                self.app.log_text()[log_from:]))
            self.expect(c, "200 console lines: the Inspector shows them with at most 4 forced layouts",
                        bool(shown) and forced <= 4,
                        "forced layouts %d; entries %d -> %d" % (forced, base, self.count(
                            self.last_probe("Inspector", log_from) or {}, ".console-entry")))
        log_from = len(self.app.log_text())
        self.hw("key", pid, 34, "cmd", "alt")
        self.expect(c, "Cmd+Opt+I again closed it", bool(self.app.wait_log(r"Closing inspector panel", 5, log_from)))
        time.sleep(1.2)
        shot = self.capture(c, "closed")
        if shot is not None:
            code, win = self.hw("window", pid)
            self.expect(c, "closed: the page is green to the bottom", self.green_at(shot, x0 + pw / 2, win["h"] - 20))
        if vid is not None:
            log_from = len(self.app.log_text())
            time.sleep(10)
            relayouts = len(re.findall(r"relayout\{id=EngineViewId\(%s\)[^\n]*relayout cause=" % vid,
                                       self.app.log_text()[log_from:]))
            self.expect(c, "closed: the hidden Inspector lays out nothing in 10 s", relayouts == 0,
                        "%d relayouts" % relayouts)

    A4_SETTINGS_BTN_JS = ("setTimeout(function () { var r = document.getElementById('sidebarSettingsBtn')"
                          ".getBoundingClientRect(); sendIpc('a4_settings_button', { rect: [r.left, r.top, "
                          "r.width, r.height] }); }, 4000);")

    def settings_path(self):
        return self.app.state_file.parent / "settings.json"

    def a4_settings_window(self, c):
        """A fresh app, RustKit chrome (the default), its settings window opened from the sidebar's button and rendered:
        {pid, shown (the first probe), at (a page point to hwdrive's), page_h}, or None."""
        if not self.a4_open(c, {"HIWAVE_RUSTKIT_CHROME_EVAL": self.A4_SETTINGS_BTN_JS}):
            return None
        m = self.app.wait_log(r'RustKit chrome IPC body=\{"cmd":"a4_settings_button","rect":\[([-\d.]+),([-\d.]+),([-\d.]+),([-\d.]+)\]', 10)
        if not self.expect(c, "the chrome reported its settings button's box", bool(m)):
            return None
        bx, by, bw, bh = (float(m.group(i)) for i in range(1, 5))
        if not self.can_input:
            self.not_run(c, "open, change, save, close", self.why_not("Accessibility"))
            return None
        pid = self.app.proc.pid
        log_from = len(self.app.log_text())
        self.hw("click", pid, int(bx + bw / 2), int(self.TITLE_BAR + by + bh / 2))
        self.expect(c, "the sidebar's settings button opened the settings window",
                    bool(self.app.wait_log(r"Opening settings window", 5, log_from)))
        self.expect(c, "the settings page was built as a RustKit view",
                    bool(self.app.wait_log(r"the settings page is a RustKit view", 5, log_from)))
        got = self.panel_ipc("settings", "get_settings", 10, log_from)
        self.expect(c, "the settings page asked for the settings (get_settings)", bool(got))
        shown = self.wait_probe("settings", lambda p: self.count(p, ".section") >= 5 and
                                self.el(p, "#sidebarDefaultOpen").get("checked") is not None, 10, log_from)
        self.expect(c, "the settings page rendered (its sections, the checkbox)", bool(shown),
                    json.dumps((shown or self.last_probe("settings", log_from) or {}).get("boxes", "no probe"))[:240])
        if not shown:
            return None
        time.sleep(1.0)
        code, wins = self.hw("windows", pid)
        wins = wins.get("windows", [])
        self.expect(c, "the app shows a second window", len(wins) >= 2 and wins[1].get("on_screen"), json.dumps(wins))
        if len(wins) < 2:
            return None
        main, sw = wins[0], wins[1]
        title = sw["h"] - shown["size"][1]
        # Points in the main window's coordinates (hwdrive's), for a point in the settings page.
        def at(px, py):
            return int(sw["x"] - main["x"] + px), int(sw["y"] - main["y"] + title + py)
        if self.can_capture:
            path = self.out / ("%s.window.png" % c)
            subprocess.run(["/usr/sbin/screencapture", "-x", "-o", "-l", str(sw["id"]), str(path)], capture_output=True)
            if path.is_file():
                from PIL import Image
                img = Image.open(path).convert("RGB")
                colours = len(img.getcolors(maxcolors=img.width * img.height + 1) or [])
                self.expect(c, "the settings window is not blank", colours > 50, "%d colours" % colours)
        return {"pid": pid, "shown": shown, "at": at, "page_h": shown["size"][1]}

    def settings_in_view(self, w, p, sel):
        b = self.el(p, sel).get("box")
        return bool(b) and b[2] > 0 and b[1] >= 0 and b[1] + b[3] <= w["page_h"]

    def settings_scroll_to(self, w, sel, p):
        """The wheel over the settings window `w` until `sel` is in view; the last probe."""
        for _ in range(14):
            if self.settings_in_view(w, p, sel):
                break
            b = self.el(p, sel).get("box")
            dy = -300 if (b and b[1] + b[3] > w["page_h"]) else 300
            start = len(self.app.log_text())
            self.hw("scroll", w["pid"], *w["at"](w["shown"]["size"][0] / 2, w["page_h"] / 2), 0, dy)
            prev = self.el(p, sel).get("box")
            p = self.wait_probe("settings", lambda q: self.el(q, sel).get("box") != prev, 2, start) or p
        return p

    def a4_settings(self):
        """CH3.A4: RustKit chrome (the default), the settings window's page is a RustKit view: the sidebar's button opens it, it shows the saved settings, takes clicks, the wheel and typing, saves a focus-blocklist domain to focus_mode.json, posts save_settings and shows the reply, and Escape closes it."""
        c = "a4_settings"
        w = self.a4_settings_window(c)
        if w is None:
            return
        pid, shown, at = w["pid"], w["shown"], w["at"]
        cb = self.el(shown, "#sidebarDefaultOpen")
        was = cb["checked"]
        log_from = len(self.app.log_text())
        # The checkbox is 'Sidebar open by default' in the first section.
        self.hw("click", pid, *at(cb["box"][0] + cb["box"][2] / 2, cb["box"][1] + cb["box"][3] / 2))
        flipped = self.wait_probe("settings", lambda p: self.el(p, "#sidebarDefaultOpen").get("checked") == (not was),
                                  4, log_from)
        self.expect(c, "a click on 'sidebar open by default' flips it", bool(flipped),
                    str(self.el(self.last_probe("settings", log_from), "#sidebarDefaultOpen")))
        cur = flipped or shown

        def box_of(p, sel):
            return self.el(p, sel).get("box")

        def in_view(p, sel):
            return self.settings_in_view(w, p, sel)

        def scroll_to(sel, p):
            return self.settings_scroll_to(w, sel, p)

        # A setting saved on its own: a domain added to the focus blocklist
        # is written to focus_mode.json at once.
        focus_path = self.settings_path().parent / "focus_mode.json"
        cur = scroll_to("#focusBlocklistInput", cur)
        self.expect(c, "the wheel over the settings window scrolls it to the focus blocklist",
                    in_view(cur, "#focusBlocklistInput") and in_view(cur, "#focusBlocklistAddBtn"),
                    "%s %s" % (box_of(cur, "#focusBlocklistInput"), box_of(cur, "#focusBlocklistAddBtn")))
        ib, ab = box_of(cur, "#focusBlocklistInput"), box_of(cur, "#focusBlocklistAddBtn")
        if ib and ab:
            listed = self.count(cur, ".blocklist-item")
            log_from = len(self.app.log_text())
            self.hw("click", pid, *at(ib[0] + 40, ib[1] + ib[3] / 2))
            time.sleep(0.4)
            self.hw("type", pid, "a4-blocked.example")
            typed = self.wait_probe("settings", lambda p: self.el(p, "#focusBlocklistInput").get("len")
                                    == len("a4-blocked.example"), 4, log_from)
            self.expect(c, "typing in the settings window goes to its input (its value is 18 long)", bool(typed),
                        str(self.el(self.last_probe("settings", log_from), "#focusBlocklistInput")))
            self.hw("click", pid, *at(ab[0] + ab[2] / 2, ab[1] + ab[3] / 2))
            add = self.panel_ipc("settings", "add_to_focus_blocklist", 5, log_from)
            self.expect(c, "Add posted add_to_focus_blocklist", bool(add), add.group(0)[:160] if add else "")
            text, end = "", time.time() + 5
            while time.time() < end and "a4-blocked.example" not in text:
                try:
                    text = focus_path.read_text()
                except FileNotFoundError:
                    text = ""
                time.sleep(0.2)
            self.expect(c, "focus_mode.json holds the added domain", "a4-blocked.example" in text, str(focus_path.name))
            shown_list = self.wait_probe("settings", lambda p: self.count(p, ".blocklist-item") == listed + 1 and
                                         self.count(p, '.blocklist-item[data-domain="a4-blocked.example"]') == 1,
                                         3, log_from)
            self.expect(c, "the page lists the added domain (one more item, the domain's)", bool(shown_list),
                        str(self.el(self.last_probe("settings", log_from), ".blocklist-item")))
        # Save at the end of the page: the app takes save_settings and its
        # reply reaches the page (a postMessage). Since B11 settings.json is
        # written, with the checkbox flipped; the next check starts on the
        # seeded profile again (open_page), not on this one.
        cur = scroll_to("#saveBtn", cur)
        self.expect(c, "the wheel over the settings window scrolls it to the Save button", in_view(cur, "#saveBtn"),
                    str(box_of(cur, "#saveBtn")))
        sbox = box_of(cur, "#saveBtn")
        if in_view(cur, "#saveBtn"):
            log_from = len(self.app.log_text())
            self.hw("click", pid, *at(sbox[0] + sbox[2] / 2, sbox[1] + sbox[3] / 2))
            sv = self.panel_ipc("settings", "save_settings", 5, log_from)
            # The body is not logged (#910 R1); the flipped checkbox is what
            # the page read into it.
            self.expect(c, "Save posted save_settings with the checkbox still flipped",
                        bool(sv) and self.el(self.last_probe("settings"), "#sidebarDefaultOpen").get("checked") == (not was),
                        sv.group(0)[:160] if sv else "")
            saved = self.wait_probe("settings", lambda p: self.count(p, ".status.show.success") >= 1, 5, log_from)
            self.expect(c, "the page showed the app's reply (settings_saved, a postMessage: its success status)",
                        bool(saved))
        log_from = len(self.app.log_text())
        self.hw("key", pid, 53)  # Escape
        cl = self.panel_ipc("settings", "close_settings", 5, log_from)
        self.expect(c, "Escape posted close_settings", bool(cl))
        self.expect(c, "the app hid the settings window", bool(self.app.wait_log(r"Closing settings window", 3, log_from)))
        time.sleep(0.6)
        code, wins = self.hw("windows", pid)
        shown_now = [w for w in wins.get("windows", []) if w.get("on_screen")]
        self.expect(c, "one window is left on screen", len(shown_now) == 1, json.dumps(wins.get("windows", [])))

    A4_SECRET = "a4-secret-7Q2"

    def a4_private(self):
        """#910 R1: RustKit chrome (the default), what the RustKit panels are given stays out of the app log: a line typed in the Inspector's console, a vault master password and a saved credential (URL with a query, username, password) are never in it; the log names their cmds only."""
        c = "a4_private"
        secret = self.A4_SECRET
        w = self.a4_settings_window(c)
        if w is None:
            return
        pid, at = w["pid"], w["at"]
        def type_into(sel, text, p):
            p = self.settings_scroll_to(w, sel, p)
            b = self.el(p, sel).get("box")
            if not self.settings_in_view(w, p, sel):
                return None, p
            self.hw("click", pid, *at(b[0] + min(b[2], 120) / 2, b[1] + b[3] / 2))
            time.sleep(0.3)
            self.hw("type", pid, text)
            got = self.wait_probe("settings", lambda q: self.el(q, sel).get("len") == len(text), 4)
            return got, (got or p)
        def click(sel, p):
            p = self.settings_scroll_to(w, sel, p)
            b = self.el(p, sel).get("box")
            if self.settings_in_view(w, p, sel):
                self.hw("click", pid, *at(b[0] + b[2] / 2, b[1] + b[3] / 2))
                return True
            return False
        # The vault: a fresh profile's is locked; unlock it with a master password.
        cur = self.wait_probe("settings", lambda p: (self.el(p, "#vaultPassword").get("box") or [0, 0, 0, 0])[2] > 0, 5) \
            or w["shown"]
        log_from = len(self.app.log_text())
        typed, cur = type_into("#vaultPassword", "pw-" + secret, cur)
        self.expect(c, "the master password went into the vault field", bool(typed),
                    str(self.el(self.last_probe("settings"), "#vaultPassword")))
        self.hw("key", pid, 36)
        un = self.panel_ipc("settings", "unlock_vault", 5, log_from)
        self.expect(c, "Return posted unlock_vault", bool(un), un.group(0) if un else "")
        cur = self.wait_probe("settings", lambda p: (self.el(p, "#newCredUrl").get("box") or [0, 0, 0, 0])[2] > 0, 6, log_from)
        self.expect(c, "the vault unlocked (the credential form shows)", bool(cur),
                    str(self.el(self.last_probe("settings"), "#newCredUrl")))
        if cur:
            log_from = len(self.app.log_text())
            ok = all(type_into(sel, text, cur)[0] for sel, text in (
                ("#newCredUrl", "https://a4.example/login?token=" + secret),
                ("#newCredUsername", "user-" + secret),
                ("#newCredPassword", "cred-" + secret)))
            self.expect(c, "the credential's three fields took the typing", ok)
            click("#addCredentialBtn", self.last_probe("settings") or cur)
            sv = self.panel_ipc("settings", "save_credential", 5, log_from)
            self.expect(c, "Save Credential posted save_credential", bool(sv), sv.group(0) if sv else "")
        self.hw("key", pid, 53)  # Escape
        self.app.wait_log(r"Closing settings window", 3)
        time.sleep(0.6)
        # The Inspector's console: a line with the secret in it.
        log_from = len(self.app.log_text())
        self.hw("key", pid, 34, "cmd", "alt")  # Cmd+Opt+I
        tree = self.wait_probe("Inspector", lambda p: (self.el(p, "[data-tab=console]").get("box") or [0, 0, 0, 0])[2] > 0
                               and p["size"][1] == 300, 10, log_from)
        if tree:
            code, win = self.hw("window", pid)
            x0, y0 = win["w"] - tree["size"][0], win["h"] - tree["size"][1]
            tab = self.el(tree, "[data-tab=console]")["box"]
            self.hw("click", pid, int(x0 + tab[0] + tab[2] / 2), int(y0 + tab[1] + tab[3] / 2))
            con = self.wait_probe("Inspector", lambda p: (self.el(p, "#console-input").get("box") or [0, 0, 0, 0])[2] > 0,
                                  5, log_from)
            if con:
                box = self.el(con, "#console-input")["box"]
                self.hw("click", pid, int(x0 + box[0] + min(box[2], 200) / 2), int(y0 + box[1] + box[3] / 2))
                time.sleep(0.3)
                self.hw("type", pid, "'code-%s' + 1" % secret)
                self.hw("key", pid, 36)
        ev = self.panel_ipc("Inspector", "inspector_console_eval", 5, log_from)
        self.expect(c, "the console posted inspector_console_eval", bool(ev), ev.group(0) if ev else "")
        time.sleep(1.0)
        text = self.app.log_text()
        hits = [l.strip()[:200] for l in text.splitlines() if secret in l]
        self.expect(c, "the app log never holds the password, the credential or the console line", not hits,
                    "%d lines: %s" % (len(hits), hits[:3]))
        panel_bodies = [l.strip()[:200] for l in text.splitlines() if "RustKit panel IPC" in l and "body=" in l]
        self.expect(c, "no RustKit panel IPC line carries a body", not panel_bodies, str(panel_bodies[:2]))

    def find(self, query):
        """hwdrive's Accessibility lookup: the element's box, or None."""
        code, got = self.hw("find", self.app.proc.pid, query)
        return got if code == 0 else None

    def b11_settings_save(self):
        """CH2.B11: the sidebar's Settings opens the settings window; a flipped 'Sidebar Open on Startup' and Save Settings write settings.json, and the page says so only when it was written."""
        c = "b11_settings_save"
        if not self.open_page(c, "e9_final.html"):
            return
        if not (self.can_input and self.pre.get("accessibility")):
            return self.not_run(c, "open, flip, save", self.why_not("Accessibility"))
        pid = self.app.proc.pid
        path = self.app.state_file.parent / "settings.json"
        try:
            before = json.loads(path.read_text())
        except (FileNotFoundError, ValueError):
            before = None

        def click(box):
            self.hw("click", pid, int(box[0] + box[2] / 2), int(box[1] + box[3] / 2))

        btn = self.find("Settings")
        if not self.expect(c, "the sidebar shows a Settings item", bool(btn), json.dumps(btn)):
            return
        log_from = len(self.app.log_text())
        click(btn["box"])
        self.expect(c, "a click on it opened the settings window",
                    bool(self.app.wait_log(r"Opening settings window", 5, log_from)))
        label = None
        end = time.time() + 5
        while time.time() < end and not label:
            label = self.find("Sidebar Open on Startup")
        if not self.expect(c, "the settings page shows 'Sidebar Open on Startup'", bool(label)):
            return
        ly = label["box"][1] + label["box"][3] / 2
        boxes = (self.find("role:AXCheckBox") or {}).get("found", [])
        row = [b for b in boxes if b["box"][0] > label["box"][0] and abs(b["box"][1] + b["box"][3] / 2 - ly) < 30]
        if not self.expect(c, "its toggle is on the same row", len(row) == 1, json.dumps(row or boxes[:4])):
            return
        was = row[0].get("value")
        click(row[0]["box"])
        time.sleep(0.5)
        now = [b for b in (self.find("role:AXCheckBox") or {}).get("found", []) if b["box"] == row[0]["box"]]
        self.expect(c, "a click flipped the toggle", bool(now) and now[0].get("value") == 1 - was,
                    "was %s, now %s" % (was, now and now[0].get("value")))
        save = self.find("Save Settings")
        if not self.expect(c, "the page has a Save Settings button", bool(save)):
            return
        click(save["box"])
        # The page shows 'Settings saved!' for 2 s after the reply.
        time.sleep(0.4)
        said_saved = bool(self.find("Settings saved!"))
        after = None
        end = time.time() + 5
        while time.time() < end:
            try:
                after = json.loads(path.read_text())
                if after != before:
                    break
            except (FileNotFoundError, ValueError):
                pass
            time.sleep(0.2)
        self.expect(c, "Save Settings wrote settings.json with the flipped toggle",
                    bool(after) and after != before and after.get("sidebar_default_open") == bool(1 - was),
                    "before %s, after %s" % (before and before.get("sidebar_default_open"),
                                             after and after.get("sidebar_default_open")))
        written = bool(after) and after != before
        self.expect(c, "the page says 'Settings saved!' only when the file was written",
                    said_saved == written and said_saved,
                    "page said saved: %s, file written: %s" % (said_saved, written))

    # ── CH3.A3c: the alpha route (a transparent chrome surface) ─────────────

    # A rounded popup with a wide shadow over the page column, 6 s after boot.
    A3C_PROBE_JS = ("setTimeout(function () { var d = document.createElement('div'); d.id = 'a3cProbe'; "
                    "d.style.cssText = 'position:absolute;z-index:9999;left:640px;top:260px;width:160px;"
                    "height:90px;background:#0000ff;border-radius:24px;box-shadow:0 12px 32px rgba(0,0,0,0.6)'; "
                    "document.body.appendChild(d); }, 6000);")

    def a3c_probe_shot(self, c):
        if not self.a3_open(c, {"HIWAVE_RUSTKIT_CHROME_EVAL": self.A3C_PROBE_JS}):
            return None
        time.sleep(5.0)
        shot = self.capture(c, "probe")
        if shot is None:
            self.not_run(c, "the probe popup over the page", self.why_not("Screen Recording"))
            return None
        img, scale, win = shot
        mid = img.getpixel((int(720 * scale), int((self.TITLE_BAR + 305) * scale)))
        self.expect(c, "the probe popup's blue shows over the page",
                    all(abs(mid[i] - BLUE[i]) <= 48 for i in range(3)), str(mid))
        return shot

    def a3c_shadow_over_page(self):
        """CH3.A3c: just outside a chrome popup's box, its shadow darkens the page (page colour blended with the shadow, not the body colour)."""
        c = "a3c_shadow_over_page"
        shot = self.a3c_probe_shot(c)
        if shot is None:
            return
        img, scale, win = shot
        # 14 px below the box's bottom edge, mid-width: inside the shadow.
        rgb = img.getpixel((int(720 * scale), int((self.TITLE_BAR + 350 + 14) * scale)))
        self.expect(c, "the pixel below the box is the page's green darkened by the shadow",
                    rgb[1] < 235 and rgb[1] > rgb[0] + 40 and rgb[1] > rgb[2] + 40, "at 720,%d: %s" % (350 + 14, rgb))
        far = img.getpixel((int(720 * scale), int((self.TITLE_BAR + 470) * scale)))
        self.expect(c, "past the shadow's reach the page is plain green", self.is_green(far, 24), str(far))

    def a3c_rounded_corner(self):
        """CH3.A3c: the pixel at a rounded chrome popup's corner shows the page, not the body colour."""
        c = "a3c_rounded_corner"
        shot = self.a3c_probe_shot(c)
        if shot is None:
            return
        img, scale, win = shot
        # One point inside the box's top-left corner: outside its 24 px curve.
        rgb = img.getpixel((int(641 * scale), int((self.TITLE_BAR + 261) * scale)))
        self.expect(c, "the corner pixel is the page's green (under the shadow), not blue or the body colour",
                    rgb[1] > rgb[0] + 60 and rgb[1] > rgb[2] + 60, "at 641,261: %s" % (rgb,))

    def a3c_find_dims_page(self):
        """CH3.A3c: with the chrome's find modal open, its translucent overlay dims the page."""
        c = "a3c_find_dims_page"
        if not self.a3_open(c, {"HIWAVE_RUSTKIT_CHROME_EVAL": "setTimeout(function () { openFindModal(); }, 6000);"}):
            return
        before = self.capture(c, "before")
        time.sleep(5.0)
        after = self.capture(c, "find_open")
        if before is None or after is None:
            return self.not_run(c, "the page dims under the find modal", self.why_not("Screen Recording"))
        img0, scale, win = before
        img1 = after[0]
        at = (int(win["w"] * 0.5 * scale), int(win["h"] * 0.7 * scale))
        p0, p1 = img0.getpixel(at), img1.getpixel(at)
        self.expect(c, "before: the page is green there", self.is_green(p0, 24), str(p0))
        self.expect(c, "with the modal open the same page pixel is darker green (0.3 black over it)",
                    p1[1] < p0[1] - 40 and p1[1] > 120 and p1[1] > p1[0] + 60, "%s -> %s" % (p0, p1))

    def a3c_chrome_band(self):
        """CH3.A3c: with the chrome's body unpainted, the strip between the toolbar and the page column still shows the body's colour (default and tab-actions heights), not the window behind the chrome."""
        c = "a3c_chrome_band"
        js = ("setTimeout(function () { var t = currentTabs && currentTabs[0]; "
              "if (t) showTabActionsPanel(t.id); }, 8000);")
        if not self.a3_open(c, {"HIWAVE_RUSTKIT_CHROME_EVAL": js}):
            return
        shots = [("default", self.capture(c, "default"), 98)]
        log_from = len(self.app.log_text())
        self.app.wait_log(r'RustKit chrome IPC body=\{"cmd":"expand_chrome_small"', 10, log_from)
        time.sleep(1.5)
        # With the tab-actions panel open the page column starts at 148; the
        # panel ends at 144, and the rows between are the body's colour.
        shots.append(("tab_actions", self.capture(c, "tab_actions"), 145))
        for name, shot, y in shots:
            if shot is None:
                self.not_run(c, "the band is the body colour (%s)" % name, self.why_not("Screen Recording"))
                continue
            img, scale, win = shot
            # The toolbar's own background at its right end is --bg-primary,
            # the body's colour.
            ref = img.getpixel((int((win["w"] - 6) * scale), int((self.TITLE_BAR + 60) * scale)))
            row = [img.getpixel((int(x * scale), int((self.TITLE_BAR + y) * scale))) for x in range(300, int(win["w"]) - 20, 97)]
            bad = [p for p in row if max(abs(p[i] - ref[i]) for i in range(3)) > 12]
            self.expect(c, "%s: the band above the page column is the body colour %s" % (name, ref), not bad,
                        "y %d: %s" % (y, row))

    # ── CH2.E6b: the Mode <select>'s drop-down over the page ───────────────

    # 4 s after boot: open the left sidebar as its toggle does, give the
    # Mode select's options long labels (values kept) so its drop-down is
    # wider than the sidebar and reaches over the page column, and report
    # the select's box and value (an IPC the driver reads from the log).
    E6B_JS = ("setTimeout(function () { if (!sidebarOpen) toggleSidebar(); "
              "var s = document.getElementById('modeSelector'); "
              "Array.prototype.forEach.call(s.options, function (o) { "
              "o.textContent = o.textContent + ' with a label long enough to reach over the page'; }); "
              "setTimeout(function () { var r = s.getBoundingClientRect(); "
              "sendIpc('e6b_select', { x: Math.round(r.left), y: Math.round(r.top), w: Math.round(r.width), "
              "h: Math.round(r.height), i: s.selectedIndex }); }, 1500); }, 4000);")
    E6B_SELECT = (r'RustKit chrome IPC body=\{"cmd":"e6b_select","x":(-?\d+),"y":(-?\d+),"w":(\d+),"h":(\d+),"i":(\d+)')

    def e6b_select_over_page(self):
        """CH2.E6b: the RustKit chrome's Mode <select> drops down over the page column, a click on an option there picks it (set_mode), and a click on the page while it is open closes it without reaching the page."""
        c = "e6b_select_over_page"
        if not self.a3_open(c, {"HIWAVE_RUSTKIT_CHROME_EVAL": self.E6B_JS}):
            return
        sel = self.app.wait_log(self.E6B_SELECT, 12)
        self.expect(c, "the chrome reported the Mode select's box", bool(sel), sel.group(0) if sel else "no report")
        if not sel:
            return
        sx, sy, sw, sh, shown = (int(sel.group(i)) for i in range(1, 6))
        if not self.can_input:
            return self.not_run(c, "open the Mode select", self.why_not("Accessibility"))
        pid = self.app.proc.pid
        time.sleep(0.8)
        before = self.capture(c, "before")
        # Open it.
        log_from = len(self.app.log_text())
        self.hw("click", pid, sx + sw // 2, self.TITLE_BAR + sy + sh // 2)
        m = self.app.wait_log(self.OVERLAY, 5, log_from)
        self.expect(c, "the open drop-down is in the chrome's region over the page column", bool(m),
                    m.group(0) if m else "no overlay")
        if not m:
            return
        x, y, w, h = (float(m.group(i)) for i in range(2, 6))
        # The overlay is the popup cut to the page column: its top is the
        # select's bottom, or the column's top when the popup starts above.
        top = sy + sh
        self.expect(c, "it hangs under the select and reaches past the sidebar",
                    y >= top - 2 and x + w > sx + sw + 20, "box %s, select %s" % ([x, y, w, h], [sx, sy, sw, sh]))
        time.sleep(0.5)
        after = self.capture(c, "open")
        if before is None or after is None:
            self.not_run(c, "the drop-down paints over the page", self.why_not("Screen Recording"))
        else:
            px, py = x + min(w / 2, 60), self.TITLE_BAR + y + 4
            self.expect(c, "before: the page is where the drop-down opens", self.green_at(before, px, py),
                        "at %d,%d" % (px, py))
            self.expect(c, "open: the drop-down paints over the page there", not self.green_at(after, px, py),
                        "at %d,%d: %s" % (px, py, after[0].getpixel((int(px * after[1]), int(py * after[1])))))
            self.expect(c, "open: the page still shows beside it", self.green_at(after, x + w + 40, py))
        # Pick another option on its row over the page column: rows are
        # round(13 * 1.2 + 4) = 20 px under a 1 px border.
        pick = 1 if shown != 1 else 0
        value = ["essentials", "balanced", "zen"][pick]
        log_from, mark = len(self.app.log_text()), self.fixture.mark()
        self.hw("click", pid, int(x + min(w / 2, 60)), int(self.TITLE_BAR + top + 1 + 20 * pick + 10))
        ipc = self.app.wait_log(r'RustKit chrome IPC body=\{"cmd":"set_mode","mode":"(\w+)"', 3, log_from)
        self.expect(c, "a click on an option over the page picks it (set_mode)", bool(ipc) and ipc.group(1) == value,
                    ipc.group(0) if ipc else "no set_mode")
        self.expect(c, "the pick closed the drop-down",
                    bool(self.app.wait_log(r"RustKit chrome overlays count=0", 3, log_from)))
        time.sleep(0.3)
        self.expect(c, "the page did not get that click",
                    "content click (view-local)" not in self.app.log_text()[log_from:]
                    and not self.fixture.seen("beacon?a3-click-", mark))
        # Open it again, then click on the page away from it.
        log_from = len(self.app.log_text())
        self.hw("click", pid, sx + sw // 2, self.TITLE_BAR + sy + sh // 2)
        if not self.app.wait_log(self.OVERLAY, 5, log_from):
            return self.expect(c, "the drop-down opened again", False)
        time.sleep(0.3)
        log_from, mark = len(self.app.log_text()), self.fixture.mark()
        code, win = self.hw("window", pid)
        self.hw("click", pid, int(win["w"] * 0.7), int(win["h"] * 0.75))
        self.expect(c, "a click on the page while it is open closes it",
                    bool(self.app.wait_log(r"RustKit chrome overlays count=0", 3, log_from)))
        time.sleep(1.0)
        self.expect(c, "and the page does not get that click (Chromium: the press only dismisses the popup)",
                    "content click (view-local)" not in self.app.log_text()[log_from:]
                    and not self.fixture.seen("beacon?a3-click-", mark), str(self.page_clicks(mark)))
        self.expect(c, "the mode did not change", not self.app.wait_log(r'"cmd":"set_mode"', 0.2, log_from))
        # The next click on the page is the page's again.
        log_from, mark = len(self.app.log_text()), self.fixture.mark()
        self.hw("click", pid, int(win["w"] * 0.7), int(win["h"] * 0.75))
        self.expect(c, "once it is closed, a click on the page reaches the page",
                    self.fixture.wait("beacon?a3-click-", 5, mark))

    # ── M1d: tab shortcuts from a focused page, exactly once ───────────────

    ACTIVATE_IPC = r'RustKit chrome IPC body=\{"cmd":"activate_tab","id":"(\d+)"\}'

    def m1d_press(self, c, what, keycode, mods, want, menu_id):
        """One chord; within 2 s the chrome posts exactly one activate_tab
        (for tab `want` if given). `menu_id`: the menu item that must (a
        string) or must not (False) take it. Returns the activated id."""
        pid, log_from = self.app.proc.pid, len(self.app.log_text())
        self.hw("key", pid, keycode, *mods)
        self.app.wait_log(self.ACTIVATE_IPC, 3, log_from)
        time.sleep(2.0)
        log = self.app.log_text()[log_from:]
        got = re.findall(self.ACTIVATE_IPC, log)
        detail = "activate_tab %s" % (got or "none")
        ok = len(got) == 1 and (want is None or got[0] == want)
        if menu_id:
            took = 'menu event id="%s"' % menu_id in log
            ok = ok and took
            detail += ", menu %s" % ("took it" if took else "did not take it")
        elif menu_id is False:
            menu = re.findall(r'menu event id="(select_[a-z_0-9]+)"', log)
            ok = ok and not menu
            detail += ", menu events %s" % (menu or "none")
        self.expect(c, "%s: switched exactly once%s" % (what, " to tab %s" % want if want else ""), ok, detail)
        return got[0] if len(got) == 1 else None

    def m1d_beacons(self, mark, needle):
        return [urllib.parse.unquote(p) for p in self.fixture.seen(needle, mark)]

    def m1d_tab_shortcuts(self):
        """M1d: five tabs, focus in a page text field: Cmd+3 the third tab, Cmd+9 the last, Ctrl+Tab the next (its Tab release swallowed even with Ctrl let go first; Ctrl+Option+Tab is the page's); with the address bar focused Cmd+2 the second, each exactly once."""
        c = "m1d_tab_shortcuts"
        started = self.e3_start(c, page="m1d_keys.html")
        if not started:
            return
        pid, w, h = started
        self.app.wait_log(r'RustKit chrome IPC body=\{"cmd":"get_workspaces"', 20)
        time.sleep(1.0)
        made = self.m1_tabs(c, 4)
        if len(made) != 4:
            return
        # The fifth tab onto the keys page, through the address bar (a new
        # tab's is empty and focused after Cmd+T).
        mark = self.fixture.mark()
        self.hw("key", pid, 37, "cmd")  # Cmd+L
        time.sleep(0.4)
        self.hw("type", pid, "%s/m1d_keys.html?tab=5" % self.fixture.base)
        self.hw("key", pid, 36)
        self.expect(c, "the fifth tab loaded the keys page", self.fixture.wait("beacon?m1d-t5-ready", 10, mark))
        time.sleep(1.0)
        if not self.e3_focus(c, pid, w, h):
            return
        self.m1d_press(c, "Cmd+3 in a page text field", 20, ["cmd"], made[1], "select_tab_3")
        self.m1d_press(c, "Cmd+9", 25, ["cmd"], made[3], "select_last_tab")
        time.sleep(1.0)
        self.e3_focus(c, pid, w, h)
        # Ctrl let go before Tab: the release carries no Ctrl (R1 nit 1).
        mark = self.fixture.mark()
        first = self.m1d_press(c, "Ctrl+Tab in a page text field, Ctrl released first (from the last: wraps to the first)",
                               48, ["ctrl", "bareup"], None, None)
        self.expect(c, "Ctrl+Tab's tab is the first (not one Cmd+T made)", first is not None and first not in made,
                    str(first))
        heard = self.m1d_beacons(mark, "beacon?m1d-t") 
        tabs = [b for b in heard if "-Tab-" in b]
        self.expect(c, "no page heard the Ctrl+Tab press or its release", not tabs, str(tabs))
        # Ctrl+Option+Tab is not the chord (R1 nit 2): no switch, the page hears it.
        self.fixture.wait("beacon?m1d-t0-ready", 5, mark)
        time.sleep(0.8)
        mark, log_from = self.fixture.mark(), len(self.app.log_text())
        self.hw("key", pid, 48, "ctrl", "alt")
        time.sleep(2.0)
        switched = re.findall(self.ACTIVATE_IPC, self.app.log_text()[log_from:])
        down = [b for b in self.m1d_beacons(mark, "beacon?m1d-t0-keydown-Tab") if "-c1-a1" in b]
        self.expect(c, "Ctrl+Option+Tab: no switch, and the page heard its keydown", not switched and bool(down),
                    "activate_tab %s, page %s" % (switched or "none", down or "heard nothing"))
        self.m1d_press(c, "Cmd+Shift+]", 30, ["cmd", "shift"], made[0], "select_next_tab")
        self.m1d_press(c, "Ctrl+Shift+Tab", 48, ["ctrl", "shift"], first, None)
        # The address bar focused: the chrome's listener switches and claims
        # the chord, so the menu does not switch again.
        self.hw("key", pid, 37, "cmd")  # Cmd+L
        time.sleep(0.6)
        self.m1d_press(c, "Cmd+2 in the address bar", 19, ["cmd"], made[0], False)
        self.expect(c, "the app is still running", self.app.alive())

    def m1d_page_keeps_cmd_digits(self):
        """M1d R1 nit 4: a page that cancels Cmd+1..9 keeps them (Chromium does not reserve them): Cmd+2 in it switches nothing and the page hears it."""
        c = "m1d_page_keeps_cmd_digits"
        started = self.e3_start(c, page="m1d_keys.html?prevent")
        if not started:
            return
        pid, w, h = started
        self.app.wait_log(r'RustKit chrome IPC body=\{"cmd":"get_workspaces"', 20)
        time.sleep(1.0)
        made = self.m1_tabs(c, 1)
        if len(made) != 1:
            return
        # From the address bar (the chrome's, which switches), back to the page.
        self.hw("key", pid, 37, "cmd")  # Cmd+L
        time.sleep(0.6)
        first = self.m1d_press(c, "Cmd+1 in the address bar", 18, ["cmd"], None, False)
        if first is None:
            return
        self.fixture.wait("beacon?m1d-t0-ready", 5, self.since)
        time.sleep(1.0)
        if not self.e3_focus(c, pid, w, h):
            return
        mark, log_from = self.fixture.mark(), len(self.app.log_text())
        self.hw("key", pid, 19, "cmd")  # Cmd+2
        time.sleep(2.0)
        log = self.app.log_text()[log_from:]
        switched = re.findall(self.ACTIVATE_IPC, log)
        heard = self.m1d_beacons(mark, "beacon?m1d-t0-keydown-2")
        self.expect(c, "Cmd+2 in a page that cancels it: no switch, no menu event, the page heard it",
                    not switched and 'menu event id="select_tab_2"' not in log and bool(heard),
                    "activate_tab %s, page %s" % (switched or "none", heard or "heard nothing"))

    CHORD_TO_MENU = re.compile(
        r"^(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?)Z\b[^\n]*Cmd chord went to the menu[^\n]*\bsince_press_ms=(\d+)", re.M)
    FOCUS_URL_EVENT = re.compile(
        r"^(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?)Z\b[^\n]*menu event id=\"focus_url\"", re.M)

    @staticmethod
    def log_epoch(stamp):
        import datetime
        return datetime.datetime.fromisoformat(stamp[:26]).replace(tzinfo=datetime.timezone.utc).timestamp()

    def m1d_cmd_l_while_loading(self):
        """M1e (Handtest 11-09): Cmd+L with focus in a page still loading (an image the server holds 8 s; then a page whose thread is busy) takes the address bar within 250 ms of the press, once; Escape then stops the load."""
        c = "m1d_cmd_l_while_loading"
        env = {"RUST_LOG": "info,rustkit_viewhost=debug"}
        for query, what in (("slowimg", "an image still loading"), ("busy", "the page's thread busy")):
            if not self.open_page(c, "m1d_keys.html?" + query, env=env):
                return
            if not self.can_input:
                return self.not_run(c, "Cmd+L", self.why_not("Accessibility"))
            self.fixture.wait("beacon?m1d-t0-ready", 10, self.since)
            pid = self.app.proc.pid
            code, win = self.hw("window", pid)
            # A click into #a: the content view takes the keyboard at once;
            # the page may hear the click only when its thread is free.
            self.hw("click", pid, win["w"] * 0.7, win["h"] * 0.35)
            time.sleep(0.4)
            # The server holds the image 8 s from its request: still held
            # with a second to spare. ("Navigation finished" is no guide:
            # the chrome's own load logs it too.)
            held = [t for (t, p) in self.fixture.requests[self.since:] if "/slowimg/" in p]
            age = (time.time() - self.fixture.t0 - held[0]) if held else None
            loading = age is not None and age < 7.0
            if query == "slowimg" and not self.expect(c, "%s: the image is still held when Cmd+L is pressed" % what, loading,
                                                      "requested %s s ago" % (round(age, 1) if age is not None else "never")):
                self.close_page()
                continue
            log_from, t = len(self.app.log_text()), time.time()
            self.hw("key", pid, 37, "cmd")
            took = self.app.wait_log(r'menu event id="focus_url"', 3, log_from)
            log = self.app.log_text()[log_from:]
            chord = self.CHORD_TO_MENU.search(log)
            focus = self.FOCUS_URL_EVENT.search(log)
            # From the press (the key event's own timestamp, logged by the
            # view as since_press_ms when the chord went to the menu) to
            # the menu's focus_url.
            ms = None
            if chord and focus:
                pressed = self.log_epoch(chord.group(1)) - int(chord.group(2)) / 1000.0
                ms = round((self.log_epoch(focus.group(1)) - pressed) * 1000, 1)
            self.expect(c, "%s: Cmd+L took the address bar within 250 ms of the press" % what,
                        bool(took) and ms is not None and ms < 250,
                        "%s ms from the press (driver wall clock %.0f ms); %s" % (
                            ms, (time.time() - t) * 1000, chord.group(0)[-160:] if chord else "no chord-to-menu line"))
            if query == "slowimg":
                # The address bar has the keyboard: Escape there is the
                # chrome's stop. The chrome's Escape closes an open sidebar
                # first (chrome.html's keydown, older than M1e), so a
                # second Escape is the stop then; the note says which.
                stop_from = len(self.app.log_text())
                self.hw("key", pid, 53)
                stopped = self.app.wait_log(r"Stopping load", 1.5, stop_from)
                presses = 1
                if not stopped and re.search(r'"cmd":"set_sidebar_width"', self.app.log_text()[stop_from:]):
                    self.note(c, "%s: the first Escape closed the sidebar" % what, "INFO")
                    self.hw("key", pid, 53)
                    presses = 2
                    stopped = self.app.wait_log(r"Stopping load", 3, stop_from)
                time.sleep(0.5)
                after = self.app.log_text()[stop_from:]
                self.expect(c, "%s: Escape then stopped the load" % what,
                            bool(stopped) and "Stop: nothing loading" not in after,
                            "%d Escape; stop: %s; %s" % (presses, bool(stopped),
                                                         "nothing loading" if "Stop: nothing loading" in after else "a load was cancelled"))
            # The page's late answer (its thread freed by the stop or between
            # busy spells) is dropped: the menu ran once.
            self.app.wait_log(r"late answer for a Cmd chord dropped", 4, log_from)
            time.sleep(0.3)
            log = self.app.log_text()[log_from:]
            runs = len(re.findall(r'menu event id="focus_url"', log))
            self.expect(c, "%s: one Cmd+L, one focus_url (the late answer did not run it again)" % what, runs == 1,
                        "%d focus_url; late answer %s" % (runs, "dropped" if "late answer for a Cmd chord dropped" in log else "not seen"))
            # Both runs write the check's one log: keep each.
            (self.out / ("%s.%s.app.log" % (c, query))).write_text(self.app.log_text())
            self.close_page()

    def m1g_cmd_l_slow_listener(self):
        """M1g (#1023 R1 item 5): a page whose Cmd+L keydown listener runs 300 ms, then one whose listener runs 2 s (neither cancels): Cmd+L reaches the address bar once, after the listener; the time from the press is reported."""
        c = "m1g_cmd_l_slow_listener"
        env = {"RUST_LOG": "info,rustkit_viewhost=debug"}
        for ms_listen in (300, 2000):
            what = "a %d ms listener" % ms_listen
            if not self.open_page(c, "m1d_keys.html?slowl=%d" % ms_listen, env=env):
                return
            if not self.can_input:
                return self.not_run(c, "Cmd+L", self.why_not("Accessibility"))
            self.fixture.wait("beacon?m1d-t0-ready", 10, self.since)
            pid = self.app.proc.pid
            code, win = self.hw("window", pid)
            self.hw("click", pid, win["w"] * 0.7, win["h"] * 0.35)
            time.sleep(0.6)
            mark = self.fixture.mark()
            log_from, t = len(self.app.log_text()), time.time()
            self.hw("key", pid, 37, "cmd")
            took = self.app.wait_log(r'menu event id="focus_url"', ms_listen / 1000.0 + 5, log_from)
            wall = (time.time() - t) * 1000
            time.sleep(0.8)
            log = self.app.log_text()[log_from:]
            chord = self.CHORD_TO_MENU.search(log)
            focus = self.FOCUS_URL_EVENT.search(log)
            ms = None
            if chord and focus:
                pressed = self.log_epoch(chord.group(1)) - int(chord.group(2)) / 1000.0
                ms = round((self.log_epoch(focus.group(1)) - pressed) * 1000, 1)
            heard = len(self.fixture.seen("beacon?m1g-slowl-done", mark)) > 0
            why = re.search(r'why="([^"]*)"', chord.group(0)) if chord else None
            self.expect(c, "%s: the page heard Cmd+L and its listener ran" % what, heard)
            self.expect(c, "%s: Cmd+L took the address bar after the listener, from the page's answer" % what,
                        bool(took) and ms is not None and ms >= ms_listen and why is not None
                        and why.group(1) == "the page did not claim it",
                        "%s ms from the press (driver wall clock %.0f ms); why: %s" % (
                            ms, wall, why.group(1) if why else "no chord-to-menu line"))
            runs = len(re.findall(r'menu event id="focus_url"', log))
            self.expect(c, "%s: one focus_url" % what, runs == 1, "%d focus_url" % runs)
            # The page answered: nothing was given up (#1023 delta HOLD).
            self.expect(c, "%s: no chord was given up" % what, "never answered" not in log,
                        "a give-up line appeared" if "never answered" in log else "none")
            self.note(c, "%s: press to address bar" % what, "INFO", "%s ms" % ms)
            (self.out / ("%s.%d.app.log" % (c, ms_listen))).write_text(self.app.log_text())
            self.close_page()

    def m1e_page_keeps_cmd_l(self):
        """M1e: a page that cancels Cmd+L's keydown in time keeps it, as in Chromium (IDC_FOCUS_LOCATION is not in kReservedCommandIds): the address bar does not take it and the page hears it."""
        c = "m1e_page_keeps_cmd_l"
        started = self.e3_start(c, page="m1d_keys.html?preventl")
        if not started:
            return
        pid, w, h = started
        self.fixture.wait("beacon?m1d-t0-ready", 5, self.since)
        if not self.e3_focus(c, pid, w, h):
            return
        mark, log_from = self.fixture.mark(), len(self.app.log_text())
        self.hw("key", pid, 37, "cmd")  # Cmd+L
        time.sleep(1.5)
        log = self.app.log_text()[log_from:]
        heard = self.m1d_beacons(mark, "beacon?m1d-t0-keydown-l")
        self.expect(c, "Cmd+L in a page that cancels it: no focus_url, the page heard it",
                    'menu event id="focus_url"' not in log and bool(heard),
                    "menu %s, page %s" % ("took it" if 'menu event id="focus_url"' in log else "did not take it",
                                          heard or "heard nothing"))

    CHECKS = ["h1", "h1_slow", "h2", "h3", "h4", "h4_slow", "h6", "h6_extent", "h8", "h16_click_nav", "h19_spin",
              "i0a_script", "i0a_layout", "i0a_nav", "i0a_close", "e9_redirect", "e9_redirect_webkit", "a6_url_log", "a2_chrome_boots", "a7_no_bridge_in_content",
              "a2_chrome_navigate", "a2_chrome_typed_url", "a3_passthrough_click", "a3_dropdown_over_page",
              "a3b_context_menu_over_page", "a3b_unlisted_popup_over_page",
              "a3c_shadow_over_page", "a3c_rounded_corner", "a3c_find_dims_page", "a3c_chrome_band",
              "e6b_select_over_page",
              "a3_cmd_l_from_page", "a3_resize", "e12_input_select_all", "e4_contextmenu",
              "e4_contextmenu_prevented", "e4_dblclick_selects_word", "e4_drag", "e4_cursor_hand", "e4_tooltip",
              "e4_chrome_tab", "e19_icon_hover", "e22_focus_ring",
              "a4_shelf", "a4_inspector", "a4_settings", "a4_private",
              "b11_settings_save",
              "e3_dead_key", "e3_dead_key_no_field", "e3_paste", "e3_cut_copy", "e3_undo", "e3_cmd_key_to_page", "e3_chrome_paste_url",
              "m1_cmd_w_repeat", "m1_cmd_w_after_clicks", "m1d_tab_shortcuts",
              "m1d_page_keeps_cmd_digits", "m1d_cmd_l_while_loading", "m1e_page_keeps_cmd_l",
              "m1g_cmd_l_slow_listener"]

    def run(self, names):
        sha = hashlib.sha256(self.binary.read_bytes()).hexdigest()
        print("app      %s\nsha256   %s\nseat     %s" % (self.binary, sha, json.dumps(self.pre, sort_keys=True)))
        for name in names:
            check = getattr(self, name)
            print("%s: %s" % (name, check.__doc__.strip()), flush=True)
            try:
                check()
            except HidUnsafe as e:
                self.results.append((name, "HID input", "NOT RUN", "stopped: %s" % e))
                print("  STOP (HID safety): %s" % e, flush=True)
                self.close_page()
                break
            finally:
                self.close_page()
        tally = {s: sum(1 for r in self.results if r[2] == s) for s in ("PASS", "FAIL", "NOT RUN")}
        summary = {
            "app": str(self.binary), "sha256": sha, "seat": self.pre, "tally": tally,
            "assertions": [dict(check=r[0], name=r[1], status=r[2], detail=r[3]) for r in self.results],
            "requests": self.fixture.requests,
        }
        (self.out / "result.json").write_text(json.dumps(summary, indent=1))
        print("PASS %(PASS)d  FAIL %(FAIL)d  NOT RUN %(NOT RUN)d" % tally, "->", self.out / "result.json")
        return 1 if tally["FAIL"] else (3 if tally["NOT RUN"] else 0)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--app", help="path to the built hiwave binary")
    ap.add_argument("--out", default=str(HERE / "out"), help="directory for logs, frames and result.json")
    ap.add_argument("--checks", default=",".join(Driver.CHECKS))
    ap.add_argument("--hwdrive", help="a prebuilt hwdrive (default: compile hwdrive.swift into --out)")
    ap.add_argument("--list", action="store_true")
    args = ap.parse_args()
    if args.list:
        for name in Driver.CHECKS:
            print(name, "-", getattr(Driver, name).__doc__.strip())
        return 0
    if not args.app or not pathlib.Path(args.app).is_file():
        print("--app must name the built hiwave binary", file=sys.stderr)
        return 2
    names = [n for n in args.checks.split(",") if n]
    unknown = [n for n in names if n not in Driver.CHECKS]
    if unknown:
        print("unknown checks: %s" % unknown, file=sys.stderr)
        return 2
    driver = Driver(args)
    try:
        return driver.run(names)
    finally:
        driver.close_page()


if __name__ == "__main__":
    sys.exit(main())
