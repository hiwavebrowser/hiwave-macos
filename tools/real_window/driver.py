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
    def __init__(self, binary, profile, log_path):
        self.binary, self.profile, self.log_path = binary, profile, log_path
        self.proc = None

    @property
    def state_file(self):
        return self.profile / "Library" / "Application Support" / "hiwave" / "workspace_state.json"

    def start(self):
        env = dict(os.environ, HOME=str(self.profile))
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

    def open_page(self, check, page):
        """A fresh app on `page`. Returns once the engine has started the load
        (or failed to), with the outcome recorded."""
        profile = self.out / "profile"
        if self.template is None:
            profile.mkdir(parents=True, exist_ok=True)
            self.seed_profile(profile)
        url = "%s/%s" % (self.fixture.base, page)
        state = json.loads(self.template)
        set_tab_urls(state, url)
        app = App(self.binary, profile, self.out / (check + ".app.log"))
        app.state_file.write_text(json.dumps(state))
        self.since = self.fixture.mark()
        self.app = app.start()
        got = self.fixture.wait("/" + page, 30, self.since)
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
        time.sleep(1.0)
        frame = self.frame(c, "after", [GREEN])
        if frame is None:
            return self.not_run(c, "the window shows the page's green image", self.why_not("Screen Recording"))
        self.expect(c, "the window shows the page's green image", frame[GREEN] > 2000, str(frame))

    CHECKS = ["h1", "h1_slow", "h2", "h3", "h4", "h4_slow", "h6", "h6_extent", "h8", "h16_click_nav", "h19_spin"]

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
