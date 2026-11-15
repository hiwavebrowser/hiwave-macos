"""h19_spin and i0a_script read the stop of the page's own script (H19).

Batch 44 (develop 28c79bff) failed both checks seven times, and none of
the seven was the engine's: the headless pins in `webview_rustkit.rs`
(`content_thread_a_load_script_that_never_ends_is_stopped_at_the_15_s_budget_and_logged`
and the `content_thread_after_a_budget_stop_*` ones) pass on that tree.

- h19_spin waited for `Navigation finished` after the page's script
  started. The RustKit chrome (the default since CH3.A5a) loads its own
  page beside the content page, on its own thread, and wrote that line
  0 s later, so the check read "the script was given 0 s" and gave up
  looking for the stop line 2 s later, 13 s before the stop.
- i0a_script looked for the window thread's hold to end. The page runs on
  the content thread, the window thread is never held, and the line never
  comes. And its Cmd+T opened a new tab, which loads the start page into
  the same content view as soon as the stop frees it, so the page it then
  asked to tick, answer its request and show green was gone.

No app, no window: a fake App holds the log the batch-44 runs wrote, in
the order they wrote it.
"""

import pathlib
import re
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))

import driver as rw  # noqa: E402


class LogApp:
    """driver.App with a fixed log and no process."""

    text = ""

    def __init__(self, binary, profile, log_path, env=None):
        self.binary, self.profile, self.log_path, self.env = binary, profile, log_path, env

    @property
    def state_file(self):
        return self.profile / "Library" / "Application Support" / "hiwave" / "workspace_state.json"

    def start(self):
        self.state_file.parent.mkdir(parents=True, exist_ok=True)
        if not self.state_file.is_file():
            self.state_file.write_text('{"tabs": [{"url": "hiwave://newtab"}]}')
        self.proc = type("P", (), {"pid": 4242})()
        return self

    def alive(self):
        return True

    def log_text(self):
        return LogApp.text

    def wait_log(self, pattern, timeout, start=0):
        return re.search(pattern, self.log_text()[start:])

    def stop(self):
        pass


class Fixture:
    base = "http://127.0.0.1:1"
    reply = b"ok"

    def mark(self):
        return 0

    def wait(self, needle, timeout, since=0, count=1):
        return True

    def seen(self, needle, since=0):
        return [needle]


def driver_at(out, monkeypatch, log):
    LogApp.text = log
    monkeypatch.setattr(rw, "App", LogApp)
    monkeypatch.setattr(rw.time, "sleep", lambda s: None)
    d = object.__new__(rw.Driver)
    d.binary, d.out, d.fixture, d.template, d.app, d.results = out / "hiwave", out, Fixture(), None, None, []
    d.can_input = d.can_resize = d.can_capture = False
    d.pre = {"screen_locked": True}
    d.sent = []

    def hw(*argv):
        d.sent.append(argv)
        return 0, {"w": 1280, "h": 832, "id": 1}

    d.hw = hw
    d.frame = lambda check, name, colours: None
    return d


def status(d, check, name):
    found = [r for r in d.results if r[0] == check and r[1] == name]
    assert found, "no assertion %r in %s" % (name, [r[1] for r in d.results])
    return found[-1][2], found[-1][3]


# The order batch 44's h19_spin.app.log has: the content page's script
# starts, the chrome's own load finishes beside it, and the page's stop
# and load end come 15 s later.
H19_LOG = """\
INFO rustkit_engine: Root box built total_children=1
INFO rustkit_engine: Running page script source=inline#1 bytes=205
INFO rustkit_engine: Running inline script of an HTML load source=inline#1 bytes=4954
INFO rustkit_core: Navigation finished navigation_id=NavigationId(1) elapsed_ms=164
WARN rustkit_engine::wgate: Content thread hold entry=load_html ms=164
WARN rustkit_js::control: Watchdog: a script turn ran past its budget; interrupting it turn=1 ran_ms=15010 budget_ms=15000
WARN rustkit_engine: Script stopped at the script budget source=inline#1 elapsed_ms=15010 late_ms=0
INFO rustkit_core: Navigation finished navigation_id=NavigationId(2) elapsed_ms=16222
"""


def test_h19_reads_the_page_s_stop_not_the_chrome_s_load(tmp_path, monkeypatch):
    d = driver_at(tmp_path, monkeypatch, H19_LOG)
    d.h19_spin()
    for name in ("the load finished within 45 s of the script's start",
                 "the script was given its budget before it was stopped",
                 "the window was not held much past the 15 s budget",
                 "the app log names the stopped script and how long it ran"):
        assert status(d, "h19_spin", name)[0] == "PASS", (name, status(d, "h19_spin", name))


def test_h19_still_fails_a_load_that_was_never_stopped(tmp_path, monkeypatch):
    log = H19_LOG.split("WARN rustkit_js::control")[0]
    d = driver_at(tmp_path, monkeypatch, log)
    d.h19_spin()
    assert status(d, "h19_spin", "the load finished within 45 s of the script's start")[0] == "FAIL"


def test_h19_still_fails_a_stop_before_the_budget(tmp_path, monkeypatch):
    d = driver_at(tmp_path, monkeypatch, H19_LOG.replace("elapsed_ms=15010", "elapsed_ms=900"))
    d.h19_spin()
    assert status(d, "h19_spin", "the script was given its budget before it was stopped")[0] == "FAIL"


I0A_LOG = """\
INFO hiwave: Window thread stall log on
INFO rustkit_engine: Root box built total_children=1
INFO hiwave: New tab - navigating to start page
WARN rustkit_engine: Script stopped at the script budget source=timers elapsed_ms=5000 late_ms=0
"""


def test_i0a_script_reads_the_content_thread_s_stop(tmp_path, monkeypatch):
    d = driver_at(tmp_path, monkeypatch, I0A_LOG)
    # The stop comes after the spin starts, so after the check's mark.
    real_start_spin = rw.Driver.start_spin

    def start_spin(self, c):
        mark = real_start_spin(self, c)
        LogApp.text = I0A_LOG.split("WARN rustkit_engine: Script stopped")[0]
        return mark

    monkeypatch.setattr(rw.Driver, "start_spin", start_spin)
    LogApp.text = I0A_LOG.split("WARN rustkit_engine: Script stopped")[0]
    orig = LogApp.wait_log

    def wait_log(self, pattern, timeout, start=0):
        LogApp.text = I0A_LOG  # the stop arrives while the check waits
        return orig(self, pattern, timeout, start)

    monkeypatch.setattr(LogApp, "wait_log", wait_log)
    d.i0a_script()
    assert status(d, "i0a_script", "the hold ended within 10 s of the script's start")[0] == "PASS"
    assert status(d, "i0a_script", "the page ticks again afterwards")[0] == "PASS"


def test_i0a_script_does_not_open_a_new_tab_over_the_page_it_measures(tmp_path, monkeypatch):
    d = driver_at(tmp_path, monkeypatch, I0A_LOG)
    d.can_input = True
    monkeypatch.setattr(rw.Driver, "hid_guard", lambda self, argv: None)
    d.i0a_script()
    keys = [argv for argv in d.sent if argv and argv[0] == "key"]
    assert not any(argv[2] == 17 for argv in keys), "i0a_script pressed Cmd+T: %s" % keys


def test_i0a_close_still_opens_a_new_tab_while_the_script_runs(tmp_path, monkeypatch):
    d = driver_at(tmp_path, monkeypatch, I0A_LOG)
    d.can_input = True
    d.i0a_close()
    keys = [argv for argv in d.sent if argv and argv[0] == "key"]
    assert any(argv[2] == 17 for argv in keys), "i0a_close no longer presses Cmd+T: %s" % keys
