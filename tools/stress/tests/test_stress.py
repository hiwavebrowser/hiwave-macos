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


# ── R1 on #1083 ────────────────────────────────────────────────────────────

class Clock:
    """A clock the guards read and their waits move: `sleep` advances it."""

    def __init__(self, hhmm):
        self.t = (hhmm // 100) * 3600.0 + (hhmm % 100) * 60.0
        self.idle_from = 0.0  # the seat is idle from this time on

    def time(self):
        return self.t

    def sleep(self, seconds):
        self.t += seconds

    def minutes(self, offset_s=0):
        return int((self.t + offset_s) // 60) % 1440

    def idle(self):
        return 9999.0 if self.t >= self.idle_from else 0.0


def moving(monkeypatch, tmp_path, hhmm):
    clock = Clock(hhmm)
    monkeypatch.setattr(run, "time", clock)
    monkeypatch.setattr(run, "eastern_minutes", clock.minutes)
    monkeypatch.setattr(run, "hid_idle_seconds", clock.idle)
    monkeypatch.setattr(run, "LOCK", tmp_path / "lock")
    return clock


def test_a_wait_for_an_idle_seat_that_runs_toward_five_is_refused_after_the_wait(monkeypatch, tmp_path):
    # 16:10, a 20-minute run: fine at entry (ends 16:30). The seat goes
    # idle at 16:45; by then the run would end at 17:05.
    clock = moving(monkeypatch, tmp_path, 1610)
    clock.idle_from = 16 * 3600 + 45 * 60
    try:
        with run.WindowLock(20 * 60, 3600):
            entered = True
    except SystemExit as e:
        entered, code = False, e.code
    assert not entered and code == 2
    assert clock.t >= clock.idle_from, "it did wait for the seat"
    assert not (tmp_path / "lock").exists()


def test_the_last_check_before_launch_reads_the_clock_again(monkeypatch, tmp_path):
    # 16:10, a 20-minute run, idle seat: the lock is given. The load wait
    # that follows lasts until 16:45: launching now could show a window at 17:05.
    clock = moving(monkeypatch, tmp_path, 1610)
    with run.WindowLock(20 * 60, 0) as lock:
        lock.clear_to_launch()  # 16:10: fine
        clock.sleep(35 * 60)
        try:
            lock.clear_to_launch()
            refused = False
        except SystemExit:
            refused = True
        assert refused
        # ... and somebody sitting down in the meantime is seen as well
        clock.t = 9 * 3600.0
        clock.idle_from = clock.t + 1
        try:
            lock.clear_to_launch()
            refused = False
        except SystemExit:
            refused = True
        assert refused
    assert not (tmp_path / "lock").exists()


def test_a_run_whose_wall_limit_crosses_five_is_refused_at_entry(monkeypatch, tmp_path):
    moving(monkeypatch, tmp_path, 1610)
    try:
        with run.WindowLock(60 * 60, 0):
            entered = True
    except SystemExit:
        entered = False
    assert not entered and not (tmp_path / "lock").exists()


def test_socket_and_log_readers_name_what_is_not_this_machine(tmp_path):
    lsof = "p123\nn127.0.0.1:5000->127.0.0.1:6000\nn*:5353\nn192.168.1.5:50000->203.0.113.7:443\nn[::1]:1->[::1]:2\n"
    assert run.off_machine_peers(lsof) == {"203.0.113.7:443"}
    log = tmp_path / "app.log"
    log.write_text("Loading URL url=http://127.0.0.1:5000/index.html\nDownloading filter list: X from https://lists.example.test/x.txt\n")
    assert run.log_off_machine(log) == ["lists.example.test"]

    class FakeSink:
        hosts = ["CONNECT accounts.google.com", "GET fonts.example.test"]
    seen = run.off_machine("chrome", FakeSink, [], [], None)
    assert seen == {"sink_proxy": ["GET fonts.example.test"]}, seen  # Chrome's own host was refused, not counted
    assert run.off_machine("hiwave", None, [], [], {"non_loopback_origins": []}) == {}


FAKE_ENGINE = '''#!/usr/bin/env python3
import sys, json, time, urllib.request
url = sys.argv[sys.argv.index("--url") + 1]
base = url.split("/index.html")[0]
urllib.request.urlopen(url).read()
EXTRA
urllib.request.urlopen(urllib.request.Request(base + "/__stress/result", data=json.dumps({"score": 42.0, "metrics": {}}).encode(), method="POST")).read()
time.sleep(30)
'''


def run_with_fake_engine(monkeypatch, tmp_path, extra):
    suite = tmp_path / "suite"
    suite.mkdir(parents=True)
    (suite / "index.html").write_text("<!doctype html><html><head></head><body></body></html>")
    engine = tmp_path / "engine.py"
    engine.write_text(FAKE_ENGINE.replace("EXTRA", extra))
    engine.chmod(0o755)
    monkeypatch.setattr(run, "check_pin", lambda s: (suite, {"version": "t", "sha": "0" * 40}))
    monkeypatch.setattr(run, "wait_for_load", lambda limit, wait: True)
    out = tmp_path / "out"
    monkeypatch.setattr(sys, "argv", ["run.py", "--suite", "speedometer", "--engine", "hiwave-headless",
                                      "--binary", str(engine), "--label", "fake", "--out", str(out), "--timeout", "20"])
    code = run.main()
    return code, json.loads(next(out.glob("*.json")).read_text())


def test_an_offline_run_is_ok_and_one_that_left_the_machine_is_invalid(monkeypatch, tmp_path):
    code, rec = run_with_fake_engine(monkeypatch, tmp_path / "clean", "")
    assert (code, rec["status"], rec["score"], rec["off_machine"]) == (0, "ok", 42.0, {}), rec
    # The engine's log names a URL that is not this machine: the run is
    # invalid, exits non-zero, and its score is kept apart.
    code, rec = run_with_fake_engine(monkeypatch, tmp_path / "leaky",
                                     'print("Loading image url=https://cdn.example.test/a.png", flush=True)')
    assert code == 1 and rec["status"] == "invalid" and rec["score"] is None and rec["discarded_score"] == 42.0, rec
    assert rec["off_machine"] == {"engine_log_urls": ["cdn.example.test"]}, rec


def test_a_real_window_run_refuses_when_the_app_would_download_its_filter_lists(monkeypatch, tmp_path):
    monkeypatch.setattr(run, "FILTER_LISTS", tmp_path / "no-such-cache")
    assert run.seed_filter_lists(tmp_path / "profile") == 0
    cache = tmp_path / "cache"
    cache.mkdir()
    (cache / "easylist.txt").write_text("||ads.example.test^\n")
    monkeypatch.setattr(run, "FILTER_LISTS", cache)
    assert run.seed_filter_lists(tmp_path / "profile") == 1
    assert (tmp_path / "profile/Library/Application Support/hiwave/filter-lists/easylist.txt").is_file()


def test_compare_is_not_a_pass_when_it_compared_nothing(tmp_path):
    record(tmp_path, "speedometer", "base", 100.0, 0)
    record(tmp_path, "speedometer", "cand", 100.0, 0)
    p = subprocess.run([sys.executable, str(HERE / "report.py"), "--compare", "base", "cnad", str(tmp_path)], capture_output=True, text=True)
    assert p.returncode == 1 and "candidate did not" in p.stdout, p.stdout  # the base has runs, the typo has none
    p = subprocess.run([sys.executable, str(HERE / "report.py"), "--compare", "bsae", "cnad", str(tmp_path)], capture_output=True, text=True)
    assert p.returncode == 2 and "nothing to compare" in p.stderr, (p.stdout, p.stderr)


def test_a_base_without_a_score_or_discarded_for_load_is_a_finding(tmp_path):
    record(tmp_path, "speedometer", "base", None, 0, status="hung")
    record(tmp_path, "speedometer", "cand", 100.0, 0)
    record(tmp_path, "motionmark", "base", 100.0, 0, load=4.0)  # the base's only run started under load
    record(tmp_path, "motionmark", "cand", 100.0, 0)
    code, rows = compare(tmp_path)
    assert code == 1
    assert rows["speedometer"]["verdict"].startswith("NO BASE SCORE") and rows["motionmark"]["verdict"].startswith("NO BASE SCORE"), rows


def test_both_arms_need_the_same_n_and_a_candidate_run_that_failed_is_a_finding(tmp_path):
    for i in range(3):
        record(tmp_path, "speedometer", "base", 100.0, i)
        record(tmp_path, "jetstream", "base", 100.0, i)
    for i in range(2):
        record(tmp_path, "speedometer", "cand", 100.0, i)  # N 3 against 2
        record(tmp_path, "jetstream", "cand", 100.0, i)
    record(tmp_path, "jetstream", "cand", None, 2, status="hung")  # 3 runs, one did not finish
    code, rows = compare(tmp_path)
    assert code == 1
    assert rows["speedometer"]["verdict"].startswith("UNEQUAL N"), rows["speedometer"]
    assert rows["jetstream"]["verdict"].startswith("CANDIDATE FAILED 1 OF 3"), rows["jetstream"]


def test_the_pin_check_does_not_read_a_git_failure_as_clean(monkeypatch, tmp_path):
    (tmp_path / "Speedometer" / ".git").mkdir(parents=True)  # a directory git cannot read as a repository
    monkeypatch.setattr(run, "SUITES_DIR", tmp_path)
    try:
        run.check_pin("speedometer")
        refused = ""
    except SystemExit as e:
        refused = str(e.code)
    assert "git could not read" in refused, refused


# ── delta R1 on #1083 ──────────────────────────────────────────────────────

def test_the_sandbox_lets_loopback_through_and_refuses_everything_else(tmp_path):
    import socket
    listener = socket.socket()
    listener.bind(("127.0.0.1", 0))
    listener.listen(1)
    port = listener.getsockname()[1]
    probe = tmp_path / "probe.py"
    # 203.0.113.1 is TEST-NET-3: the sandbox refuses the connect before a packet is made.
    probe.write_text(
        "import socket, sys\n"
        "def dial(host, port):\n"
        "    s = socket.socket(); s.settimeout(3)\n"
        "    try:\n"
        "        s.connect((host, port)); return 'connected'\n"
        "    except OSError as e:\n"
        "        return type(e).__name__\n"
        "def resolve(name):\n"
        "    try:\n"
        "        socket.getaddrinfo(name, 80); return 'resolved'\n"
        "    except OSError as e:\n"
        "        return type(e).__name__\n"
        "print(dial('127.0.0.1', int(sys.argv[1])), dial('203.0.113.1', 80), resolve('stress-control.invalid'))\n")
    launcher = run.sandboxed(sys.executable, tmp_path)
    p = subprocess.run([str(launcher), str(probe), str(port)], capture_output=True, text=True, timeout=30)
    listener.close()
    assert p.stdout.split() == ["connected", "PermissionError", "gaierror"], (p.stdout, p.stderr)


def test_a_failed_socket_sample_is_not_a_clean_one_and_an_unobserved_run_is_invalid(monkeypatch, tmp_path):
    class Answer:
        def __init__(self, code, out="", err=""):
            self.returncode, self.stdout, self.stderr = code, out, err
    real = run.subprocess.run
    monkeypatch.setattr(run.subprocess, "run", lambda *a, **k: Answer(1))
    assert run.lsof_sockets(1) == ""  # no sockets: an answer
    monkeypatch.setattr(run.subprocess, "run", lambda *a, **k: Answer(1, err="lsof: status error"))
    assert run.lsof_sockets(1) is None
    monkeypatch.setattr(run.subprocess, "run", real)
    # A whole run in which lsof never answers: the score is not kept.
    monkeypatch.setattr(run, "lsof_sockets", lambda pid: None)
    code, rec = run_with_fake_engine(monkeypatch, tmp_path / "blind", "")
    assert code == 1 and rec["status"] == "invalid" and rec["score"] is None, rec
    assert "unobserved" in rec["off_machine"], rec


def test_the_filter_list_copy_takes_only_the_two_real_files(monkeypatch, tmp_path):
    cache = tmp_path / "cache"
    cache.mkdir()
    (cache / "easylist.txt").write_text("||ads.example.test^\n")
    secret = tmp_path / "secret.txt"
    secret.write_text("not a filter list")
    (cache / "easyprivacy.txt").symlink_to(secret)   # a link is not followed
    (cache / "notes.txt").write_text("some other file")  # another name is not copied
    monkeypatch.setattr(run, "FILTER_LISTS", cache)
    assert run.seed_filter_lists(tmp_path / "profile") == 1
    dest = tmp_path / "profile/Library/Application Support/hiwave/filter-lists"
    assert sorted(f.name for f in dest.iterdir()) == ["easylist.txt"]


def test_the_webkit_chrome_escape_hatch_is_refused(monkeypatch, tmp_path):
    engine = tmp_path / "engine"
    engine.write_text("#!/bin/sh\n")
    engine.chmod(0o755)
    monkeypatch.setattr(sys, "argv", ["run.py", "--suite", "control", "--engine", "hiwave", "--binary", str(engine),
                                      "--env", "HIWAVE_RUSTKIT_CHROME=0", "--out", str(tmp_path / "out")])
    try:
        run.main()
        refused = ""
    except SystemExit as e:
        refused = str(e.code)
    assert "HIWAVE_RUSTKIT_CHROME=0 is not allowed" in refused
