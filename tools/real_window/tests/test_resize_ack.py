"""The i0a resize assertions read hwdrive's own acknowledgement (T36).

Until T36, `hwdrive resize` set the window's size, slept a fixed 400 ms and
read the window back, and `expect_controls` timed that whole subprocess
against 200 ms: on an idle page it read 450-520 ms however fast the app
was (TRIAGE-i0a-2026-11-09, batch 47). hwdrive now polls the window server
for the asked size and prints its own figures in its JSON line:

    {"ack_ms": 18.4, "ax_error": 0, "ax_ms": 1.2, "bounds_ms": 18.4, "window": {...}}

`ack_ms` is null when the window did not reach the size within 2 s. The
assertion is on `ack_ms`; the subprocess's wall time is reported beside it.

No app, no window: a fake hw answers with the JSON line, and a fixed log
stands in for the app's.
"""

import datetime
import json
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))

import driver as rw  # noqa: E402
from test_budget_stop_checks import LogApp, driver_at, status  # noqa: E402

RESIZE = "while the first page loads: a resize is answered within 200 ms"


# ── the parser ─────────────────────────────────────────────────────────────

def test_ack_line_is_read():
    line = '{"ack_ms":18.4,"ax_error":0,"ax_ms":1.2,"bounds_ms":18.4,"window":{"w":1160,"h":752}}'
    assert rw.resize_ack(json.loads(line)) == (18.4, 1.2)


def test_ack_null_means_the_window_never_reached_the_size():
    line = '{"ack_ms":null,"ax_error":0,"ax_ms":0.9,"bounds_ms":null,"window":{"w":1280,"h":832}}'
    assert rw.resize_ack(json.loads(line)) == (None, 0.9)


def test_an_old_hwdrive_reports_no_ack():
    # The pre-T36 line: no timing at all, only the AX error and the window.
    assert rw.resize_ack({"ax_error": 0, "window": {"w": 1160}}) == (None, None)
    assert rw.resize_ack({"error": "no accessibility window for pid 1"}) == (None, None)
    assert rw.resize_ack(None) == (None, None)


def test_integer_ack_and_non_numbers():
    assert rw.resize_ack({"ack_ms": 7, "ax_ms": 1}) == (7.0, 1.0)
    assert rw.resize_ack({"ack_ms": True, "ax_ms": "1"}) == (None, None)


# ── the log figure, for information ────────────────────────────────────────

def stamp(t):
    return datetime.datetime.fromtimestamp(t, datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.%fZ")


def relayout(t, view, cause="resize"):
    return ("%s  INFO relayout{id=EngineViewId(%d) view_id=EngineViewId(%d)}:relayout{id=EngineViewId(%d)}: "
            "rustkit_engine: relayout cause=%s causes={%s:1} took_ms=1.5\n" % (stamp(t), view, view, view, cause, cause))


def test_relayouts_are_each_view_s_first_resize_after_the_request():
    t0 = 1_800_000_000.0
    log = (relayout(t0 - 1.0, 4)            # an earlier resize: not this one
           + relayout(t0 + 0.004, 4, "style")
           + relayout(t0 + 0.006, 4)
           + relayout(t0 + 0.031, 3)
           + relayout(t0 + 0.500, 4))       # a second one: the first counts
    assert rw.resize_relayouts(log, t0) == {4: 6.0, 3: 31.0}


def test_relayouts_none_yet():
    assert rw.resize_relayouts("INFO hiwave: Window thread stall log on\n", 0.0) == {}


# ── the assertion ──────────────────────────────────────────────────────────

def nav_driver(tmp_path, monkeypatch, resize_out, wall_s=0.0):
    d = driver_at(tmp_path, monkeypatch, "INFO hiwave: Window thread stall log on\n")
    d.can_input = d.can_resize = True
    d.app = LogApp(None, tmp_path, None).start()
    monkeypatch.setattr(rw.Driver, "hid_guard", lambda self, argv: None)
    size = {"w": 1280, "h": 832}

    def hw(*argv):
        d.sent.append(argv)
        if argv[0] == "resize":
            if wall_s:
                clock[0] += wall_s  # the subprocess's wall time
            size.update(w=argv[2], h=argv[3])
            return 0, resize_out
        return 0, dict(size, id=1)

    clock = [1_800_000_000.0]
    real_time = rw.time.time

    def fake_time():
        clock[0] += 0.001
        return clock[0] if clock[0] < 1_800_000_100.0 else real_time()

    monkeypatch.setattr(rw.time, "time", fake_time)
    d.hw = hw
    return d


def test_the_resize_is_judged_on_hwdrive_s_ack_not_the_subprocess_wall(tmp_path, monkeypatch):
    # Batch 47's shape: the subprocess took 514 ms, the window server had
    # the new size 18 ms after the request.
    d = nav_driver(tmp_path, monkeypatch, {"ack_ms": 18.4, "ax_ms": 1.2, "ax_error": 0}, wall_s=0.514)
    d.expect_controls("i0a_nav", "while the first page loads", new_tab=False)
    got, detail = status(d, "i0a_nav", RESIZE)
    assert got == "PASS", detail
    assert "ack 18.4 ms" in detail and "subprocess wall 51" in detail, detail


def test_a_slow_ack_still_fails(tmp_path, monkeypatch):
    d = nav_driver(tmp_path, monkeypatch, {"ack_ms": 230.0, "ax_ms": 34.0, "ax_error": 0})
    d.expect_controls("i0a_nav", "while the first page loads", new_tab=False)
    assert status(d, "i0a_nav", RESIZE)[0] == "FAIL"


def test_no_ack_fails_whatever_the_wall_time(tmp_path, monkeypatch):
    for out in ({"ack_ms": None, "ax_ms": 1.0, "ax_error": 0}, {"ax_error": 0}):
        d = nav_driver(tmp_path, monkeypatch, out)
        d.expect_controls("i0a_nav", "while the first page loads", new_tab=False)
        assert status(d, "i0a_nav", RESIZE)[0] == "FAIL", out


def test_i0a_nav_does_not_open_a_new_tab_over_the_pages_it_measures(tmp_path, monkeypatch):
    d = driver_at(tmp_path, monkeypatch, "INFO hiwave: Window thread stall log on\n")
    d.can_input = True
    monkeypatch.setattr(rw.Driver, "hid_guard", lambda self, argv: None)
    d.fixture.times = lambda needle, since=0: []
    d.i0a_nav()
    keys = [argv for argv in d.sent if argv and argv[0] == "key"]
    assert not any(argv[2] == 17 for argv in keys), "i0a_nav pressed Cmd+T: %s" % keys
    assert not any("a new tab (Cmd+T)" in r[1] for r in d.results), d.results


def test_i0a_layout_still_opens_a_new_tab(tmp_path, monkeypatch):
    d = driver_at(tmp_path, monkeypatch, "INFO hiwave: Window thread stall log on\n")
    d.can_input = True
    monkeypatch.setattr(rw.Driver, "hid_guard", lambda self, argv: None)
    d.i0a_layout()
    keys = [argv for argv in d.sent if argv and argv[0] == "key"]
    assert any(argv[2] == 17 for argv in keys), "i0a_layout no longer presses Cmd+T: %s" % keys
