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


# ── R1 delta: the page view's frame, timed from hwdrive's request ──────────

REQ = 1_800_000_050.0
LAYOUT = "while the page lays out: the page is laid out at the new size within 200 ms"
LAYOUT_RESIZE = "while the page lays out: a resize is answered within 200 ms"


def page_log(*relayouts):
    return ("%s  INFO rustkit_engine: Loading URL id=EngineViewId(4) url=http://127.0.0.1:1/i0a_layout.html replace=false\n"
            "%s  INFO rustkit_engine: Loading HTML content id=EngineViewId(3) len=259514\n"
            % (stamp(REQ - 5), stamp(REQ - 5))) + "".join(relayouts)


def test_req_epoch_is_read():
    assert rw.resize_request_epoch({"req_epoch": REQ}) == REQ
    assert rw.resize_request_epoch({"ack_ms": 3.0}) is None
    assert rw.resize_request_epoch(None) is None


def test_the_page_view_is_the_one_the_fixture_url_loaded_into():
    assert rw.page_view_id(page_log(), "http://127.0.0.1:1") == 4
    assert rw.page_view_id(page_log(), "http://127.0.0.1:2") is None


def layout_driver(tmp_path, monkeypatch, log, out):
    d = nav_driver(tmp_path, monkeypatch, dict({"ack_ms": 20.0, "ax_ms": 1.0, "ax_error": 0, "req_epoch": REQ}, **out))
    LogApp.text = log
    return d


def test_i0a_layout_asserts_the_page_frame_within_the_limit(tmp_path, monkeypatch):
    d = layout_driver(tmp_path, monkeypatch, page_log(relayout(REQ + 0.031, 3), relayout(REQ + 0.150, 4)), {})
    d.expect_controls("i0a_layout", "while the page lays out", frame="assert")
    got, detail = status(d, "i0a_layout", LAYOUT)
    assert got == "PASS" and "150 ms after the request; page view 4" in detail, detail


def test_a_slow_page_frame_fails_honestly(tmp_path, monkeypatch):
    # Today's expensive-layout figure (triage: 134-239 ms) fails at 239 ms.
    d = layout_driver(tmp_path, monkeypatch, page_log(relayout(REQ + 0.031, 3), relayout(REQ + 0.239, 4)), {})
    d.expect_controls("i0a_layout", "while the page lays out", frame="assert")
    assert status(d, "i0a_layout", LAYOUT)[0] == "FAIL"


def test_the_chrome_s_frame_does_not_stand_in_for_the_page_s(tmp_path, monkeypatch):
    d = layout_driver(tmp_path, monkeypatch, page_log(relayout(REQ + 0.031, 3)), {})
    d.expect_controls("i0a_layout", "while the page lays out", frame="assert")
    got, detail = status(d, "i0a_layout", LAYOUT)
    assert got == "FAIL" and "no relayout cause=resize" in detail, detail


def test_the_frame_is_timed_from_hwdrive_s_request_not_the_driver_s_t0(tmp_path, monkeypatch):
    # The driver's clock here reads 50 s before hwdrive's request (the fake
    # clock starts at REQ - 50). The page laid out 150 ms after the request:
    # that passes; timed from the driver's t0 it would read about 50 s.
    d = layout_driver(tmp_path, monkeypatch, page_log(relayout(REQ + 0.150, 4)), {})
    d.expect_controls("i0a_layout", "while the page lays out", frame="assert")
    assert status(d, "i0a_layout", LAYOUT)[0] == "PASS"


def test_no_req_epoch_fails_the_frame_closed(tmp_path, monkeypatch):
    d = nav_driver(tmp_path, monkeypatch, {"ack_ms": 20.0, "ax_ms": 1.0, "ax_error": 0})
    LogApp.text = page_log(relayout(REQ + 0.010, 4))
    d.expect_controls("i0a_layout", "while the page lays out", frame="assert")
    got, detail = status(d, "i0a_layout", LAYOUT)
    assert got == "FAIL" and "no req_epoch" in detail, detail


def test_held_pages_report_the_frame_as_info_only(tmp_path, monkeypatch):
    d = layout_driver(tmp_path, monkeypatch, page_log(relayout(REQ + 0.900, 4)), {})
    d.expect_controls("i0a_nav", "while the first page loads", new_tab=False)
    assert not any("laid out at the new size" in r[1] for r in d.results), d.results
    got, detail = status(d, "i0a_nav", RESIZE)
    assert got == "PASS" and "info, not asserted" in detail, detail


def test_a_non_dict_hwdrive_answer_fails_instead_of_crashing(tmp_path, monkeypatch):
    d = nav_driver(tmp_path, monkeypatch, None)
    d.expect_controls("i0a_nav", "while the first page loads", new_tab=False)
    got, detail = status(d, "i0a_nav", RESIZE)
    assert got == "FAIL" and "not reported" in detail, detail


def test_the_resize_comes_before_cmd_t(tmp_path, monkeypatch):
    # Cmd+T loads the start page into the content view; the frame must be
    # the page under test's (the first T36 run timed the start page's).
    d = layout_driver(tmp_path, monkeypatch, page_log(relayout(REQ + 0.100, 4)), {})
    d.app.wait_log = lambda pattern, timeout, start=0: True
    d.expect_controls("i0a_layout", "while the page lays out", frame="assert")
    order = [argv[0] if argv[0] != "key" else "key%d" % argv[2] for argv in d.sent if argv[0] in ("resize", "key")]
    assert order == ["resize", "key17"], order
