"""The i0a key-shortcut assertions time the app's answer, not hwdrive (T37).

Until T37, `expect_controls` timed Cmd+T and Cmd+W from before the
`hwdrive key` subprocess was spawned to the moment the driver's 100 ms log
poll saw the app's line. That counted the spawn, hwdrive's two 30 ms
post sleeps and up to 100 ms of polling, none of it the app's: i0a_close
read FAIL 308/266 ms and then PASS 157/160 ms between two runs with no
change (T36 #1008 disclosure). The same class as T36's resize.

`hwdrive key` now prints `req_epoch`, the wall-clock time (Unix seconds,
the clock the app log's UTC stamps use) taken just before the key-down
event is posted. The answer is the app's own line, timed by its stamp
from `req_epoch`. The subprocess's wall time is reported beside it.

No app, no window: a fake hw answers with the JSON line, and a fixed log
stands in for the app's.
"""

import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))

import driver as rw  # noqa: E402
from test_budget_stop_checks import LogApp, driver_at, status  # noqa: E402
from test_resize_ack import stamp  # noqa: E402

REQ = 1_800_000_050.0
NEW_TAB = "while the script runs: a new tab (Cmd+T) is answered within 200 ms"
CLOSE = "while the script runs: closing the tab (Cmd+W) is answered within 200 ms"


def new_tab_line(t):
    return "%s  INFO hiwave: New tab - navigating to start page\n" % stamp(t)


def closed_line(t):
    return "%s  INFO hiwave::ipc::commands: Closed tab: TabId(7)\n" % stamp(t)


def key_driver(tmp_path, monkeypatch, log, key_out, wall_s=0.0):
    """A driver whose hw answers `key` with `key_out(code)` after `wall_s`
    seconds of subprocess wall time (on a fake clock that starts 50 s
    before REQ, so the driver's own t0 is never near hwdrive's)."""
    d = driver_at(tmp_path, monkeypatch, log)
    d.can_input = True
    d.can_resize = False
    d.app = LogApp(None, tmp_path, None).start()
    monkeypatch.setattr(rw.Driver, "hid_guard", lambda self, argv: None)
    clock = [REQ - 50.0]
    real_time = rw.time.time

    def fake_time():
        clock[0] += 0.001
        return clock[0] if clock[0] < REQ + 100.0 else real_time()

    monkeypatch.setattr(rw.time, "time", fake_time)
    monkeypatch.setattr(rw.time, "sleep", lambda s: clock.__setitem__(0, clock[0] + s))

    def hw(*argv):
        d.sent.append(argv)
        if argv[0] == "key":
            clock[0] += wall_s
            return 0, key_out(argv[2])
        return 0, {"w": 1280, "h": 832, "id": 1}

    d.hw = hw
    return d


def both(t17, t13):
    return lambda code: {"key": code, "req_epoch": t17 if code == 17 else t13}


# ── the parser ─────────────────────────────────────────────────────────────

def test_key_req_epoch_is_read():
    assert rw.key_request_epoch({"key": 17, "req_epoch": REQ}) == REQ
    assert rw.key_request_epoch({"key": 17}) is None
    assert rw.key_request_epoch({"key": 17, "req_epoch": True}) is None
    assert rw.key_request_epoch(None) is None


def test_the_answer_is_the_first_matching_line_stamped_after_the_request():
    log = (new_tab_line(REQ - 2.0)          # an earlier Cmd+T: not this one
           + new_tab_line(REQ + 0.042)
           + new_tab_line(REQ + 0.500))
    assert rw.log_answer_ms(log, r"New tab - navigating to start page", REQ) == 42.0
    assert rw.log_answer_ms(log, r"Closed tab", REQ) is None
    assert rw.log_answer_ms("INFO hiwave: New tab - navigating to start page\n",
                            r"New tab - navigating to start page", REQ) is None


# ── the assertion ──────────────────────────────────────────────────────────

def test_keys_are_judged_from_hwdrive_s_request_to_the_app_s_line_not_the_subprocess_wall(tmp_path, monkeypatch):
    # The flip's shape: the subprocess and the poll took over 300 ms, the
    # app logged its answer 40 and 35 ms after each key went down.
    log = new_tab_line(REQ + 0.040) + closed_line(REQ + 1.035)
    d = key_driver(tmp_path, monkeypatch, log, both(REQ, REQ + 1.0), wall_s=0.31)
    d.expect_controls("i0a_close", "while the script runs", close=True)
    got, detail = status(d, "i0a_close", NEW_TAB)
    assert got == "PASS", detail
    assert "app answered 40 ms after the key" in detail and "subprocess wall 31" in detail, detail
    got, detail = status(d, "i0a_close", CLOSE)
    assert got == "PASS", detail
    assert "app answered 35 ms after the key" in detail, detail


def test_a_slow_app_answer_still_fails_however_fast_the_subprocess(tmp_path, monkeypatch):
    log = new_tab_line(REQ + 0.230) + closed_line(REQ + 1.020)
    d = key_driver(tmp_path, monkeypatch, log, both(REQ, REQ + 1.0))
    d.expect_controls("i0a_close", "while the script runs", close=True)
    got, detail = status(d, "i0a_close", NEW_TAB)
    assert got == "FAIL" and "app answered 230 ms after the key" in detail, detail
    assert status(d, "i0a_close", CLOSE)[0] == "PASS"


def test_no_req_epoch_fails_closed(tmp_path, monkeypatch):
    # An old hwdrive (no req_epoch) cannot be timed: FAIL, never a pass on
    # the subprocess wall.
    log = new_tab_line(REQ + 0.010) + closed_line(REQ + 1.010)
    d = key_driver(tmp_path, monkeypatch, log, lambda code: {"key": code})
    d.expect_controls("i0a_close", "while the script runs", close=True)
    for name in (NEW_TAB, CLOSE):
        got, detail = status(d, "i0a_close", name)
        assert got == "FAIL" and "no req_epoch" in detail, detail


def test_no_answer_line_fails(tmp_path, monkeypatch):
    d = key_driver(tmp_path, monkeypatch, closed_line(REQ + 1.010), both(REQ, REQ + 1.0))
    d.expect_controls("i0a_close", "while the script runs", close=True)
    got, detail = status(d, "i0a_close", NEW_TAB)
    assert got == "FAIL" and "no log line" in detail, detail


def test_a_line_stamped_before_the_request_is_not_the_answer(tmp_path, monkeypatch):
    # A line the app wrote before the key went down (an earlier tab) is
    # not this key's answer, even past the log offset.
    log = new_tab_line(REQ - 0.050) + closed_line(REQ + 1.010)
    d = key_driver(tmp_path, monkeypatch, log, both(REQ, REQ + 1.0))
    d.expect_controls("i0a_close", "while the script runs", close=True)
    assert status(d, "i0a_close", NEW_TAB)[0] == "FAIL"
