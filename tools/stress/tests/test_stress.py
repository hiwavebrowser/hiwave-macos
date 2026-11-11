"""The stress harness without a browser: the landing rule, the server's half
of the page channel, and the score readers.

    python3 -m pytest tools/stress/tests -q
"""

import json
import pathlib
import subprocess
import sys
import threading
import urllib.request
from http.server import ThreadingHTTPServer

HERE = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))

import run  # noqa: E402


def record(tmp, suite, label, score, n, status="ok", partial=False, load=2.0):
    r = {"schema": 1, "suite": suite, "engine": "hiwave", "label": label, "status": status, "score": score,
         "partial": partial, "binary_sha1": label * 5, "suite_sha": "0" * 40, "wall_s": 60.0,
         "load_avg_before": [load, 2.0, 2.0], "sub_scores": {}}
    (tmp / ("%s_%s_%d.json" % (suite, label, n))).write_text(json.dumps(r))


def compare(tmp, *extra):
    p = subprocess.run([sys.executable, str(HERE / "report.py"), "--compare", "base", "cand", "--json", str(tmp), *extra],
                       capture_output=True, text=True)
    return p.returncode, {r["suite"]: r for r in json.loads(p.stdout)}


def test_a_drop_past_the_floor_is_a_regression(tmp_path):
    for i, s in enumerate([100.0, 100.5, 101.0]):
        record(tmp_path, "speedometer", "base", s, i)
    for i, s in enumerate([96.0, 96.5, 97.0]):  # median -4.0 %, base spread 1 %
        record(tmp_path, "speedometer", "cand", s, i)
    code, rows = compare(tmp_path)
    assert code == 1 and rows["speedometer"]["verdict"] == "REGRESSION", rows
    assert rows["speedometer"]["threshold_pct"] == 3.0


def test_a_drop_inside_the_bases_own_spread_is_noise(tmp_path):
    for i, s in enumerate([95.0, 100.0, 104.0]):  # spread 9 %
        record(tmp_path, "motionmark", "base", s, i)
    for i, s in enumerate([93.0, 94.0, 95.0]):  # median -6 %
        record(tmp_path, "motionmark", "cand", s, i)
    code, rows = compare(tmp_path)
    assert code == 0 and rows["motionmark"]["verdict"] == "within noise", rows
    assert abs(rows["motionmark"]["threshold_pct"] - 9.0) < 1e-9


def test_a_candidate_that_stops_scoring_is_a_regression(tmp_path):
    record(tmp_path, "jetstream", "base", 50.0, 0)
    record(tmp_path, "jetstream", "cand", None, 0, status="hung")
    code, rows = compare(tmp_path)
    assert code == 1 and rows["jetstream"]["verdict"].startswith("REGRESSION"), rows


def test_partial_runs_never_enter_a_full_comparison(tmp_path):
    record(tmp_path, "speedometer", "base", 100.0, 0)
    record(tmp_path, "speedometer", "cand", 100.0, 0)
    record(tmp_path, "speedometer", "cand", 10.0, 1, partial=True)
    code, rows = compare(tmp_path)
    assert code == 0 and rows["speedometer"]["cand"]["n"] == 1, rows


def test_a_run_started_under_load_keeps_its_record_but_not_its_score(tmp_path):
    record(tmp_path, "speedometer", "base", 100.0, 0)
    record(tmp_path, "speedometer", "cand", 100.0, 0)
    record(tmp_path, "speedometer", "cand", 50.0, 1, load=3.0)  # started at load 3: discarded
    code, rows = compare(tmp_path)
    assert code == 0 and rows["speedometer"]["cand"]["n"] == 1 and rows["speedometer"]["cand"]["discarded"] == 1, rows


def test_nothing_starts_at_or_above_the_load_limit(monkeypatch):
    monkeypatch.setattr(run.os, "getloadavg", lambda: (6.5, 6.0, 6.0))
    assert run.wait_for_load(run.START_LOAD, 0) is False
    monkeypatch.setattr(run.os, "getloadavg", lambda: (2.9, 6.0, 6.0))
    assert run.wait_for_load(run.SCORE_LOAD, 0) is True


def test_the_server_adds_the_reporter_and_takes_the_result(tmp_path):
    (tmp_path / "index.html").write_text("<!doctype html><html><head><title>t</title></head><body></body></html>")
    rec = run.Record()
    httpd = ThreadingHTTPServer(("127.0.0.1", 0), run.make_handler(tmp_path, rec, {"/index.html"}, "<script>INJECTED</script>"))
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    base = "http://127.0.0.1:%d" % httpd.server_address[1]
    try:
        html = urllib.request.urlopen(base + "/index.html?startAutomatically=true").read().decode()
        assert html.index("<head>") < html.index("INJECTED") < html.index("<title>")
        assert (tmp_path / "index.html").read_text().count("INJECTED") == 0  # the file on disk is untouched
        assert b"__stress" in urllib.request.urlopen(base + "/__stress/reporter.js").read()
        for path, body in (("/__stress/progress", {"event": "start"}), ("/__stress/error", {"message": "m"}),
                           ("/__stress/result", {"score": 12.5, "metrics": {}})):
            urllib.request.urlopen(urllib.request.Request(base + path, data=json.dumps(body).encode(), method="POST")).read()
        urllib.request.urlopen(base + "/__stress/done?score=12.5").read()
    finally:
        httpd.shutdown()
    assert rec.progress[0]["event"] == "start" and rec.errors[0]["message"] == "m"
    assert rec.result["score"] == 12.5 and rec.done_score == "12.5"
    assert run.scores("speedometer", rec)[0] == 12.5


def test_jetstream_score_is_the_geomean_of_its_report():
    rec = run.Record()
    rec.jetstream_report = {"JetStream2.0": {"metrics": {"Score": ["Geometric"]}, "tests": {
        "a": {"metrics": {"Score": {"current": [4.0]}}, "tests": {}},
        "b": {"metrics": {"Score": {"current": [9.0]}}, "tests": {}}}}}
    score, sub, _ = run.scores("jetstream", rec)
    assert abs(score - 6.0) < 1e-9 and sub == {"a": 4.0, "b": 9.0}


def test_no_window_between_five_and_half_past_seven_eastern(monkeypatch, tmp_path):
    monkeypatch.setattr(run, "LOCK", tmp_path / "lock")
    monkeypatch.setattr(run, "hid_idle_seconds", lambda: 9999.0)
    for now, ok in ((16 * 60, False),      # a 30-minute run would not cross 17:00, a 2-hour one would
                    (17 * 60 + 5, False), (19 * 60 + 29, False), (19 * 60 + 30, True), (9 * 60, True)):
        monkeypatch.setattr(run, "eastern_minutes", lambda offset_s=0, now=now: (now + offset_s // 60) % 1440)
        limit = 2 * 3600 if now == 16 * 60 else 1800
        try:
            with run.WindowLock(limit, 0):
                entered = True
        except SystemExit:
            entered = False
        assert entered == ok, (now, entered)
        assert not (tmp_path / "lock").exists()  # released either way
