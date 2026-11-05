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

RED, GREEN, BLUE = (255, 0, 0), (0, 255, 0), (0, 0, 255)
MAGENTA, CYAN = (255, 0, 255), (0, 255, 255)


# ── fixture server ──────────────────────────────────────────────────────────

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
        env = dict(os.environ, HOME=str(self.profile), HIWAVE_TEST_HOOKS="1", **self.env)
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

    def hw(self, *argv):
        r = subprocess.run([str(self.hwdrive)] + [str(a) for a in argv], capture_output=True, text=True)
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
        profile = self.out / "profile"
        if self.template is None:
            profile.mkdir(parents=True, exist_ok=True)
            self.seed_profile(profile)
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
        started = time.time()
        m = self.app.wait_log(r"Navigation finished", 45, running.end())
        took = time.time() - started
        self.expect(c, "the load finished within 45 s of the script's start", bool(m),
                    "%.0f s; app alive: %s" % (took, self.app.alive()))
        if not m:
            return
        self.expect(c, "the script was given its budget before it was stopped", took > 10, "%.0f s" % took)
        # The window is frozen while the script runs, so the budget is also
        # the longest freeze a page can cause: it must not creep back up.
        self.expect(c, "the window was not held much past the 15 s budget", took < 25, "%.0f s" % took)
        # The log says so itself: which script, how long the window was held.
        stopped = self.app.wait_log(r"Script stopped at the script budget source=(\S+) elapsed_ms=(\d+) late_ms=(\d+)",
                                    2, running.end())
        self.expect(c, "the app log names the stopped script and how long it ran",
                    bool(stopped) and 10000 < int(stopped.group(2)) < 25000,
                    stopped.group(0) if stopped else "no 'Script stopped at the script budget' line")
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

    def expect_controls(self, c, during, close=False):
        """A new tab, a resize and (if asked) closing the tab, each answered
        within LIMIT_MS. Written without a seat that can post input: expect to
        fix these the first time they run."""
        if not self.can_input:
            for what in ["a new tab (Cmd+T)", "a resize"] + (["closing the tab (Cmd+W)"] if close else []):
                self.not_run(c, "%s: %s is answered within %d ms" % (during, what, self.LIMIT_MS),
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

        key(17, r"New tab - navigating to start page", "a new tab (Cmd+T)")
        if self.can_resize:
            code, win = self.hw("window", pid)
            w, h = int(win.get("w", 1200)) - 120, int(win.get("h", 800)) - 80
            t0 = time.time()
            code, out = self.hw("resize", pid, w, h)
            took = time.time() - t0
            code2, now = self.hw("window", pid)
            self.expect(c, "%s: a resize is answered within %d ms" % (during, self.LIMIT_MS),
                        code == 0 and took < limit and abs(now.get("w", 0) - w) < 4,
                        "%.0f ms; asked %dx%d, window %s" % (took * 1000, w, h, json.dumps(now)))
        else:
            self.not_run(c, "%s: a resize is answered within %d ms" % (during, self.LIMIT_MS),
                         self.why_not("Accessibility"))
        if close:
            key(13, r"Closed tab", "closing the tab (Cmd+W)")

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
        """I0(a): a script that does not end leaves the window usable."""
        c = "i0a_script"
        log_from = self.start_spin(c)
        if self.app is None or not self.app.alive():
            return
        self.expect_controls(c, "while the script runs")
        time.sleep(5)
        self.expect_no_hold(c, "5 s of the script running", log_from)
        # The lesser claim, while script still runs on the window's thread:
        # the hold ends. The app stops the script at its turn budget (5 s).
        if log_from is not None:
            ended = self.app.wait_log(r"Window thread held ms=(\d+)", 5, log_from)
            self.expect(c, "the hold ended within 10 s of the script's start", bool(ended),
                        ended.group(0) if ended else "still held: %s ms" % self.holds(log_from))
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
        self.expect_controls(c, "while the page lays out")
        self.expect_no_hold(c, "the load and six layouts", log_from)

    def i0a_nav(self):
        """I0(a): pages left while they are still loading leave the window usable."""
        c = "i0a_nav"
        if not self.open_page(c, "i0a_nav.html?n=1", self.STALL_ENV):
            return
        log_from = self.stall_log_on(c)
        self.expect_controls(c, "while the first page loads")
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
        c = "e9_redirect"
        if not self.open_page(c, "e9_start"):
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

    def a2_chrome_boots(self):
        """CH3.A2: with HIWAVE_RUSTKIT_CHROME=1 the chrome is a RustKit view whose script reaches the app."""
        c = "a2_chrome_boots"
        if not self.open_page(c, "e9_final.html", env={"HIWAVE_RUSTKIT_CHROME": "1"}):
            return
        m = self.app.wait_log(r"the chrome is a RustKit view", 10)
        self.expect(c, "the app built the chrome as a RustKit view", bool(m))
        # What the WebKit chrome posts at boot: its sidebar width, its workspaces.
        tabs = self.app.wait_log(r'RustKit chrome IPC body=\{"cmd":"get_sidebar_width"', 20)
        self.expect(c, "the chrome's script posted get_sidebar_width and the app's handler took it", bool(tabs),
                    tabs.group(0) if tabs else "")
        spaces = self.app.wait_log(r'RustKit chrome IPC body=\{"cmd":"get_workspaces"', 5)
        self.expect(c, "the chrome's script posted get_workspaces", bool(spaces))
        time.sleep(3)
        self.expect(c, "the app is still running 3 s later", self.app.alive())

    def a2_chrome_navigate(self):
        """CH3.A2 3c: the address bar's navigate, run in the RustKit chrome, reaches the app and the page loads it."""
        c = "a2_chrome_navigate"
        target = "%s/h16_b.html?from=a2" % self.fixture.base
        # The call the address bar's Enter handler makes. Not hiwave.navigate():
        # chrome.html replaces the bridge's window.hiwave with its own object.
        env = {"HIWAVE_RUSTKIT_CHROME": "1",
               "HIWAVE_RUSTKIT_CHROME_EVAL": "sendIpc('navigate', { url: %s })" % json.dumps(target)}
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
        """CH3.A2 granted gate: flag on, the tab strip and toolbar are painted, and a URL typed in the address bar navigates."""
        c = "a2_chrome_typed_url"
        if not self.open_page(c, "e9_final.html", env={"HIWAVE_RUSTKIT_CHROME": "1"}):
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


    # ── CH3.A3: the RustKit chrome over the page ───────────────────────────

    TITLE_BAR = 32  # the window's own title bar, above the chrome view

    def a3_open(self, c, env=None):
        """A fresh app, flag on, on a3_page.html, with the chrome booted and stacked."""
        env = dict({"HIWAVE_RUSTKIT_CHROME": "1"}, **(env or {}))
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
            g0 = sum(n for (n, p) in before[0].getcolors(maxcolors=1 << 24) if self.is_green(p))
            g1 = sum(n for (n, p) in after[0].getcolors(maxcolors=1 << 24) if self.is_green(p))
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
        """A fresh app, flag on, panels probed, on a3_page.html (green, beacons on click/key)."""
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
        """CH3.A4: flag on, the shelf (command palette) is a RustKit view: opens, renders its commands, runs one typed command, closes."""
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
        """CH3.A4: flag on, the Inspector is a RustKit view: Cmd+Opt+I opens it, it renders the page's tree, shows a page console line, evaluates a typed line, and closes."""
        c = "a4_inspector"
        if not self.a4_open(c):
            return
        if not self.can_input:
            return self.not_run(c, "Cmd+Opt+I", self.why_not("Accessibility"))
        pid = self.app.proc.pid
        log_from = len(self.app.log_text())
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
        log_from = len(self.app.log_text())
        self.hw("key", pid, 34, "cmd", "alt")
        self.expect(c, "Cmd+Opt+I again closed it", bool(self.app.wait_log(r"Closing inspector panel", 5, log_from)))
        time.sleep(1.2)
        shot = self.capture(c, "closed")
        if shot is not None:
            code, win = self.hw("window", pid)
            self.expect(c, "closed: the page is green to the bottom", self.green_at(shot, x0 + pw / 2, win["h"] - 20))

    A4_SETTINGS_BTN_JS = ("setTimeout(function () { var r = document.getElementById('sidebarSettingsBtn')"
                          ".getBoundingClientRect(); sendIpc('a4_settings_button', { rect: [r.left, r.top, "
                          "r.width, r.height] }); }, 4000);")

    def settings_path(self):
        return self.app.state_file.parent / "settings.json"

    def a4_settings_window(self, c):
        """A fresh app, flag on, its settings window opened from the sidebar's button and rendered:
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
        """CH3.A4: flag on, the settings window's page is a RustKit view: the sidebar's button opens it, it shows the saved settings, takes clicks, the wheel and typing, saves a focus-blocklist domain to focus_mode.json, posts save_settings and shows the reply, and Escape closes it."""
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
        # reply reaches the page (a postMessage). settings.json is not
        # written: the page's settings lack focus_duration_minutes and
        # focus_break_minutes, so the app's UserSettings does not parse
        # them (both backends; the PR lists it).
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
        """#910 R1: flag on, what the RustKit panels are given stays out of the app log: a line typed in the Inspector's console, a vault master password and a saved credential (URL with a query, username, password) are never in it; the log names their cmds only."""
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

    CHECKS = ["h1", "h1_slow", "h2", "h3", "h4", "h4_slow", "h6", "h6_extent", "h8", "h16_click_nav", "h19_spin",
              "i0a_script", "i0a_layout", "i0a_nav", "i0a_close", "e9_redirect", "a2_chrome_boots",
              "a2_chrome_navigate", "a2_chrome_typed_url", "a3_passthrough_click", "a3_dropdown_over_page",
              "a3_cmd_l_from_page", "a3_resize", "a4_shelf", "a4_inspector", "a4_settings", "a4_private"]

    def run(self, names):
        sha = hashlib.sha256(self.binary.read_bytes()).hexdigest()
        print("app      %s\nsha256   %s\nseat     %s" % (self.binary, sha, json.dumps(self.pre, sort_keys=True)))
        for name in names:
            check = getattr(self, name)
            print("%s: %s" % (name, check.__doc__.strip()), flush=True)
            try:
                check()
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
