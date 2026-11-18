"""m1_cmd_w_after_clicks clicks tabs where the chrome says they are (T38).

Trace #243: the check's second step walked the tab strip's row left to
right from x=232 until a click closed a tab. Every tab's block button
(`.tab-block`, "block this domain and close tab") sits just left of its
close button and takes clicks even while it is transparent, and the strip
scrolls its active tab to the centre and clips the tab at its left edge.
So the walk met a block button before any close button and the step was
NOT RUN, every time.

The chrome now reports its tabs' boxes (ids and boxes only) through
`HIWAVE_RUSTKIT_CHROME_EVAL`, and the check clicks a background tab's
title and the active tab's close button from that report.

No app, no window: `Strip` is the chrome's strip, laid out as chrome.html
lays it out (the active tab centred, clipped at the strip's left edge),
and the fake hw answers clicks and keys into the log the way the app does.
"""

import json
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))

import driver as rw  # noqa: E402
from test_budget_stop_checks import LogApp, driver_at, status  # noqa: E402

CHECK = "m1_cmd_w_after_clicks"
TITLE_BAR = 32


class Strip:
    """The RustKit chrome's tab strip: tabs `TAB_W` wide from the strip's
    left edge, scrolled so the active tab is centred (within the content),
    clipped at the strip's edges. A tab's block button is left of its close
    button; both take clicks wherever the pointer is."""

    X, Y, W, H = 220, 12, 500, 32
    TAB_W = 180

    def __init__(self, ids, active, report=True):
        self.ids, self.active, self.next_id, self.report = list(ids), active, max(ids) + 1, report

    def scroll(self):
        content = len(self.ids) * self.TAB_W
        at = self.ids.index(self.active) * self.TAB_W + self.TAB_W / 2 - self.W / 2
        return max(0, min(at, max(0, content - self.W)))

    def boxes(self):
        left, out = self.X - self.scroll(), []
        for i, tab in enumerate(self.ids):
            x = left + i * self.TAB_W
            out.append({"id": str(tab), "box": [x, self.Y, self.TAB_W, self.H], "active": tab == self.active,
                        "block": [x + self.TAB_W - 42, self.Y + 8, 16, 16],
                        "close": [x + self.TAB_W - 24, self.Y + 8, 16, 16]})
        return out

    def hit(self, x, y):
        """What a click at chrome point (x, y) lands on: (kind, tab id) or None."""
        if not (self.X <= x < self.X + self.W and self.Y <= y < self.Y + self.H):
            return None
        for tab in self.boxes():
            bx, _, bw, _ = tab["box"]
            if not bx <= x < bx + bw:
                continue
            for kind, cmd in (("block", "block_and_close"), ("close", "close_tab")):
                kx, ky, kw, kh = tab[kind]
                if kx <= x < kx + kw and ky <= y < ky + kh:
                    return cmd, tab["id"]
            return "activate_tab", tab["id"]
        return None

    def report_line(self):
        if not self.report:
            return ""
        body = {"cmd": "m1_tabs", "strip": [self.X, self.Y, self.W, self.H], "tabs": self.boxes()}
        return "INFO hiwave_app: RustKit chrome IPC body=%s\n" % json.dumps(body, separators=(",", ":"))

    def close(self, tab):
        i = self.ids.index(tab)
        self.ids.remove(tab)
        if self.active == tab and self.ids:
            self.active = self.ids[min(i, len(self.ids) - 1)]


def ipc(cmd, tab):
    return 'INFO hiwave_app: RustKit chrome IPC body={"cmd":"%s","id":"%s"}\n' % (cmd, tab)


def strip_driver(tmp_path, monkeypatch, strip):
    d = driver_at(tmp_path, monkeypatch, "INFO rustkit_engine: Root box built total_children=1\n"
                  'INFO hiwave_app: RustKit chrome IPC body={"cmd":"get_workspaces"}\n' + strip.report_line())
    d.can_input = True
    d.clicks = []

    def log(text):
        LogApp.text += text

    def hw(*argv):
        d.sent.append(argv)
        if argv[0] == "click":
            x, y = argv[2], argv[3] - TITLE_BAR
            d.clicks.append((x, y))
            got = strip.hit(x, y)
            if got:
                cmd, tab = got
                log(ipc(cmd, tab))
                if cmd == "activate_tab":
                    strip.active = int(tab)
                else:
                    strip.close(int(tab))
                    log("INFO hiwave_app: Closed tab: TabId(%s)\n" % tab)
                log(strip.report_line())
        elif argv[0] == "key" and argv[3] == "cmd" and argv[2] == 17:
            tab = strip.next_id
            strip.next_id += 1
            strip.ids.append(tab)
            strip.active = tab
            log("INFO hiwave_app: Created tab: TabId(%d)\n" % tab + strip.report_line())
        elif argv[0] == "key" and argv[3] == "cmd" and argv[2] == 13:
            tab = strip.active
            log(ipc("close_tab", tab))
            strip.close(tab)
            log("INFO hiwave_app: Closed tab: TabId(%d)\n" % tab + strip.report_line())
        return 0, {"w": 1280, "h": 832, "id": 1}

    d.hw = hw
    return d


# ── the report ─────────────────────────────────────────────────────────────

def test_the_report_is_read_from_the_chrome_ipc_line():
    strip = Strip([1, 2, 3, 4], 4)
    got = rw.m1_strip_report("INFO x: other\n" + strip.report_line())
    assert got["strip"] == [220, 12, 500, 32]
    assert [t["id"] for t in got["tabs"]] == ["1", "2", "3", "4"]
    assert [t["id"] for t in got["tabs"] if t["active"]] == ["4"]


def test_the_last_report_counts():
    first, strip = Strip([1, 2], 2).report_line(), Strip([1, 2, 3], 1)
    assert [t["id"] for t in rw.m1_strip_report(first + strip.report_line())["tabs"]] == ["1", "2", "3"]


def test_no_report_or_a_broken_one_is_none():
    assert rw.m1_strip_report("INFO x: nothing\n") is None
    assert rw.m1_strip_report('INFO hiwave_app: RustKit chrome IPC body={"cmd":"m1_tabs","strip":[1,2\n') is None
    assert rw.m1_strip_report('INFO hiwave_app: RustKit chrome IPC body={"cmd":"m1_tabs","tabs":"x"}\n') is None


def test_the_background_tab_point_is_on_its_title_inside_the_strip():
    strip = Strip([1, 2, 3, 4], 4)
    report = rw.m1_strip_report(strip.report_line())
    tab, (x, y) = rw.m1_background_tab_point(report)
    assert tab != "4"
    assert strip.hit(x, y) == ("activate_tab", tab)


def test_a_clipped_tab_is_clicked_on_its_visible_title_or_skipped():
    # Tab 1 is wholly left of the strip, tab 2 shows only its buttons.
    strip = Strip([1, 2, 3, 4, 5], 5)
    report = rw.m1_strip_report(strip.report_line())
    tab, (x, y) = rw.m1_background_tab_point(report)
    assert strip.hit(x, y) == ("activate_tab", tab)
    assert Strip.X <= x < Strip.X + Strip.W


def test_no_background_tab_on_the_strip_is_none():
    report = rw.m1_strip_report(Strip([1], 1).report_line())
    assert rw.m1_background_tab_point(report) is None


def test_the_close_point_is_the_active_tab_s_close_button():
    strip = Strip([1, 2, 3, 4], 2)
    report = rw.m1_strip_report(strip.report_line())
    tab, (x, y) = rw.m1_close_point(report)
    assert tab == "2"
    assert strip.hit(x, y) == ("close_tab", "2")


def test_a_close_button_outside_the_strip_is_none():
    report = {"strip": [220, 12, 500, 32],
              "tabs": [{"id": "9", "box": [600, 12, 180, 32], "active": True,
                        "block": [738, 20, 16, 16], "close": [756, 20, 16, 16]}]}
    assert rw.m1_close_point(report) is None


# ── the check ──────────────────────────────────────────────────────────────

def test_the_check_closes_by_the_close_button_not_the_block_button(tmp_path, monkeypatch):
    # The Trace #243 strip: after Cmd+T x3 and a click, the first tab is
    # clipped at the strip's left edge, and a walk from x=232 meets its
    # block button before any close button.
    strip = Strip([1], 1)
    d = strip_driver(tmp_path, monkeypatch, strip)
    d.m1_cmd_w_after_clicks()
    assert status(d, CHECK, "a click on a background tab activates it")[0] == "PASS"
    assert status(d, CHECK, "a click on a tab's close button closes it")[0] == "PASS"
    assert status(d, CHECK, "Cmd+W after the close button: the chrome sent close_tab and the app closed it")[0] == "PASS"
    assert "block_and_close" not in LogApp.text
    assert not [r for r in d.results if r[0] == CHECK and r[2] not in ("PASS",)], d.results


def test_the_check_clicks_where_the_report_says_not_a_sweep(tmp_path, monkeypatch):
    strip = Strip([1], 1)
    d = strip_driver(tmp_path, monkeypatch, strip)
    d.m1_cmd_w_after_clicks()
    assert len(d.clicks) == 2, d.clicks


def test_the_check_passes_the_reporter_to_the_chrome(tmp_path, monkeypatch):
    d = strip_driver(tmp_path, monkeypatch, Strip([1], 1))
    d.m1_cmd_w_after_clicks()
    js = d.app.env["HIWAVE_RUSTKIT_CHROME_EVAL"]
    assert "m1_tabs" in js and "#tabs .tab" in js


def test_no_report_is_not_run_and_clicks_nothing(tmp_path, monkeypatch):
    # A chrome that never reports (the WebKit chrome, or no test hooks).
    d = strip_driver(tmp_path, monkeypatch, Strip([1], 1, report=False))
    d.m1_cmd_w_after_clicks()
    assert d.clicks == []
    assert [r for r in d.results if r[0] == CHECK and r[2] == "NOT RUN"], d.results
