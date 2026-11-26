"""b11's Settings lookup fails on the RustKit chrome, and the report says why (T39).

b11_settings_save finds the sidebar's Settings item with `hwdrive find`, an
Accessibility lookup written for the WebKit chrome. The RustKit chrome (the
default since #939) exposes no AX tree yet (A8), so the lookup finds nothing
and the assertion fails on every run (Trace #243). The FAIL is honest and
stays a FAIL until A8 lands; a4_settings covers the same flow by other means.

What changes is the report: the assertion is labelled A8-blocked on the
console line, in result.json and in the tally, so a batch reader does not
take it for a regression. The label is narrow: only that assertion, only as
a FAIL, only on the RustKit chrome. When the lookup starts to pass, the
detail says the label can go.

No app, no window: a fake App and a fake hwdrive answer the lookups.
"""

import json
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))

import driver as rw  # noqa: E402
from test_budget_stop_checks import driver_at, status  # noqa: E402

CHECK = "b11_settings_save"
SETTINGS = "the sidebar shows a Settings item"
LOG = "INFO rustkit_engine: Root box built total_children=1\n"


def b11(tmp_path, monkeypatch, found=None, env=None, chrome_env=None):
    """Run b11 with a fake hwdrive whose `find` answers `found` (None = no
    element, as on the RustKit chrome)."""
    d = driver_at(tmp_path, monkeypatch, LOG)
    d.can_input = True
    d.pre = {"accessibility": True, "screen_locked": False}
    if chrome_env is not None:
        monkeypatch.setenv("HIWAVE_RUSTKIT_CHROME", chrome_env)
    else:
        monkeypatch.delenv("HIWAVE_RUSTKIT_CHROME", raising=False)
    if env is not None:
        real_open = d.open_page
        d.open_page = lambda c, page, e=None: real_open(c, page, env)

    def hw(*argv):
        d.sent.append(argv)
        if argv[0] == "find":
            return (0, found) if found is not None else (1, {"error": "no element"})
        return 0, {"w": 1280, "h": 832, "id": 1}

    d.hw = hw
    d.b11_settings_save()
    return d


def labels(d):
    return [r for r in d.results if len(r) > 4 and r[4]]


# ── the label ──────────────────────────────────────────────────────────────

def test_the_rustkit_chrome_fail_stays_a_fail_and_is_labelled_a8(tmp_path, monkeypatch, capsys):
    d = b11(tmp_path, monkeypatch)
    st, detail = status(d, CHECK, SETTINGS)
    assert st == "FAIL"
    assert "A8-blocked" in detail
    assert [(r[0], r[1], r[4]) for r in labels(d)] == [(CHECK, SETTINGS, "A8")]
    out = capsys.readouterr().out
    line = [l for l in out.splitlines() if SETTINGS in l][0]
    assert line.strip().startswith("FAIL") and "A8-blocked" in line


def test_the_label_reaches_result_json_and_the_tally(tmp_path, monkeypatch):
    d = b11(tmp_path, monkeypatch)
    tally, summary = d.summarise()
    # Still counted as a fail: the exit code and FAIL total do not move.
    assert tally["FAIL"] >= 1
    assert tally["blocked"] == {"A8": 1}
    rows = [a for a in summary["assertions"] if a["name"] == SETTINGS]
    assert rows[0]["status"] == "FAIL" and rows[0]["blocked_by"] == "A8"
    others = [a for a in summary["assertions"] if a["name"] != SETTINGS]
    assert all(a["blocked_by"] is None for a in others)
    json.dumps(summary)


def test_the_summary_line_names_the_blocked_fails(tmp_path, monkeypatch):
    d = b11(tmp_path, monkeypatch)
    tally, _ = d.summarise()
    assert rw.tally_line(tally) == "PASS %d  FAIL %d (1 A8-blocked)  NOT RUN %d" % (
        tally["PASS"], tally["FAIL"], tally["NOT RUN"])
    assert rw.tally_line({"PASS": 3, "FAIL": 0, "NOT RUN": 1, "blocked": {}}) == "PASS 3  FAIL 0  NOT RUN 1"


def test_the_webkit_chrome_fail_is_not_labelled(tmp_path, monkeypatch):
    # The WebKit chrome has an AX tree: a miss there is a real fail.
    d = b11(tmp_path, monkeypatch, env=rw.WEBKIT_CHROME)
    st, detail = status(d, CHECK, SETTINGS)
    assert st == "FAIL" and "A8" not in detail
    assert labels(d) == []


def test_the_webkit_chrome_from_the_runner_env_is_not_labelled(tmp_path, monkeypatch):
    d = b11(tmp_path, monkeypatch, chrome_env="0")
    assert status(d, CHECK, SETTINGS)[0] == "FAIL"
    assert labels(d) == []


def test_other_fails_are_not_labelled(tmp_path, monkeypatch):
    d = driver_at(tmp_path, monkeypatch, LOG)
    d.app = rw.App(d.binary, tmp_path, tmp_path / "x.log")
    d.expect(CHECK, "a click on it opened the settings window", False)
    d.expect("a4_settings", SETTINGS, False)
    assert labels(d) == []
    assert d.summarise()[0]["blocked"] == {}


def test_a_pass_says_the_label_can_go(tmp_path, monkeypatch):
    box = {"box": [10, 300, 40, 40]}
    d = b11(tmp_path, monkeypatch, found=box)
    st, detail = status(d, CHECK, SETTINGS)
    assert st == "PASS"
    assert "A8" in detail and "drop" in detail
    assert labels(d) == []


def test_every_result_row_has_the_label_field(tmp_path, monkeypatch):
    # Every row carries the field, labelled or not.
    d = b11(tmp_path, monkeypatch)
    assert all(len(r) == 5 for r in d.results)
