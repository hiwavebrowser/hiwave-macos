"""The stress harness without a browser: the landing rule, the server's half
of the page channel, and the score readers.

    python3 -m pytest tools/stress/tests -q
"""

import json
import os
import pathlib
import subprocess
import sys
import threading
import urllib.request
from http.server import ThreadingHTTPServer

HERE = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))

import run  # noqa: E402


def record(tmp, suite, label, score, n, status="ok", partial=False, load=2.0, budget=None):
    r = {"schema": 1, "suite": suite, "engine": "hiwave", "label": label, "status": status, "score": score,
         "partial": partial, "binary_sha1": label * 5, "suite_sha": "0" * 40, "wall_s": 60.0,
         "load_avg_before": [load, 2.0, 2.0], "sub_scores": {}, "live_turn_budget_ms": budget}
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

    def idle(self):
        return 9999.0 if self.t >= self.idle_from else 0.0


def moving(monkeypatch, tmp_path, hhmm):
    clock = Clock(hhmm)
    monkeypatch.setattr(run, "time", clock)
    monkeypatch.setattr(run, "hid_idle_seconds", clock.idle)
    monkeypatch.setattr(run, "LOCK", tmp_path / "lock")
    return clock


def test_a_run_at_half_past_five_with_an_idle_seat_is_allowed(monkeypatch, tmp_path):
    # There is no time-of-day rule (the evening no-window rule was removed 2026-11-16): at any hour,
    # for any wall limit, an idle seat gets the lock and the last check before launch passes.
    assert not hasattr(run, "QUIET") and not hasattr(run.WindowLock, "check_quiet")
    for hhmm, limit in ((1730, 30 * 60), (1610, 2 * 3600), (1929, 60), (900, 1800), (2359, 4 * 3600)):
        moving(monkeypatch, tmp_path, hhmm)
        with run.WindowLock(limit, 0) as lock:
            assert (tmp_path / "lock").is_dir()
            lock.clear_to_launch()
        assert not (tmp_path / "lock").exists(), hhmm


def test_a_busy_seat_is_waited_for_and_the_run_starts_once_it_is_idle(monkeypatch, tmp_path):
    # 16:10: somebody is at the Mac until 16:45. The lock waits for the seat, then the run goes
    # ahead, whatever the hour has become.
    clock = moving(monkeypatch, tmp_path, 1610)
    clock.idle_from = 16 * 3600 + 45 * 60
    with run.WindowLock(20 * 60, 3600) as lock:
        assert clock.t >= clock.idle_from, "it did wait for the seat"
        lock.clear_to_launch()
    assert not (tmp_path / "lock").exists()


def test_a_seat_that_stays_busy_past_the_wait_is_refused_and_the_lock_released(monkeypatch, tmp_path):
    clock = moving(monkeypatch, tmp_path, 1610)
    clock.idle_from = clock.t + 2 * 3600
    try:
        with run.WindowLock(20 * 60, 600):
            entered = True
    except SystemExit as e:
        entered, code = False, e.code
    assert not entered and code == 2
    assert clock.t >= 16 * 3600 + 10 * 60 + 600, "it waited as long as it was allowed"
    assert not (tmp_path / "lock").exists()


def test_the_last_check_before_launch_reads_the_seat_again(monkeypatch, tmp_path):
    # An idle seat: the lock is given. During the load wait that follows somebody sits down:
    # the check immediately before the window goes up sees it.
    clock = moving(monkeypatch, tmp_path, 1610)
    with run.WindowLock(20 * 60, 0) as lock:
        lock.clear_to_launch()  # idle: fine
        clock.sleep(35 * 60)
        lock.clear_to_launch()  # still idle, 35 minutes later: fine at any hour
        clock.idle_from = clock.t + 1
        try:
            lock.clear_to_launch()
            refused = False
        except SystemExit:
            refused = True
        assert refused
    assert not (tmp_path / "lock").exists()


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


def test_the_record_says_whose_wait_the_headless_live_loop_uses(monkeypatch):
    class Done:
        def __init__(self, out):
            self.stdout = out
    helps = {"new": b"  --live-ms <LIVE_MS>\n  --window-thread-pacing\n", "old": b"  --live-ms <LIVE_MS>\n",
             "other": b"usage: something else\n"}
    monkeypatch.setattr(run.subprocess, "run", lambda argv, **kw: Done(helps[argv[0]]))
    assert run.headless_pacing("new") == "content-thread"
    assert run.headless_pacing("old") == "window-thread"
    assert run.headless_pacing("other") is None

    def missing(argv, **kw):
        raise OSError("no such file")
    monkeypatch.setattr(run.subprocess, "run", missing)
    assert run.headless_pacing("gone") is None


# The first line goes to stderr (the run's engine log) before any import beyond sys and time, so a
# run that never reached the runner says whether the fake engine started at all, and when (FLK8).
FAKE_ENGINE = '''#!/usr/bin/env python3
import sys, time
sys.stderr.write("fake engine started at %.3f\\n" % time.time()); sys.stderr.flush()
import json, urllib.request
url = sys.argv[sys.argv.index("--url") + 1]
base = "/".join(url.split("/")[:3])
NETLOG
urllib.request.urlopen(url).read()
EXTRA
urllib.request.urlopen(urllib.request.Request(base + "/__stress/result", data=json.dumps({"score": 42.0, "metrics": {}}).encode(), method="POST")).read()
time.sleep(30)
'''


def run_with_fake_engine(monkeypatch, tmp_path, extra, request_log=True, suite_name="speedometer", more_argv=()):
    suite = tmp_path / "suite"
    suite.mkdir(parents=True)
    for page in ("index.html", "MotionMark/index.html"):
        (suite / page).parent.mkdir(exist_ok=True)
        (suite / page).write_text("<!doctype html><html><head></head><body></body></html>")
    engine = tmp_path / "engine.py"
    netlog = 'print("INFO hiwave::console: net start GET " + url + " status=- issued nav=1", flush=True)' if request_log else ""
    engine.write_text(FAKE_ENGINE.replace("EXTRA", extra).replace("NETLOG", netlog))
    engine.chmod(0o755)
    monkeypatch.setattr(run, "check_pin", lambda s: (suite, {"version": "t", "sha": "0" * 40}))
    monkeypatch.setattr(run, "wait_for_load", lambda limit, wait: True)
    out = tmp_path / "out"
    monkeypatch.setattr(sys, "argv", ["run.py", "--suite", suite_name, "--engine", "hiwave-headless",
                                      "--binary", str(engine), "--label", "fake", "--out", str(out), "--timeout", "20",
                                      *more_argv])
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


def test_the_checks_environment_is_applied_last_and_cannot_be_switched_off():
    asked = {"HIWAVE_RUSTKIT_CHROME": "off", "HIWAVE_CONTENT_THREAD": "0", "HIWAVE_CONSOLE_LOG": "0", "RUST_LOG": "error", "KEEP": "me"}
    env, dropped = run.engine_env(asked, inherited={"HIWAVE_CONTENT_THREAD": "0"})
    assert env["HIWAVE_RUSTKIT_CHROME"] == "1" and env["HIWAVE_CONSOLE_LOG"] == "1" and "hiwave=info" in env["RUST_LOG"]
    assert "HIWAVE_CONTENT_THREAD" not in env and dropped == ["HIWAVE_CONTENT_THREAD"] and env["KEEP"] == "me"
    # ... and a variable only the shell had is dropped too
    assert run.engine_env({}, inherited={"HIWAVE_CONTENT_THREAD": "false"})[1] == ["HIWAVE_CONTENT_THREAD"]


def test_a_run_is_unobserved_without_its_request_log_or_on_the_webkit_chrome(monkeypatch, tmp_path):
    runner = "http://127.0.0.1:5000/index.html?startAutomatically=true"
    seen = "INFO hiwave: Chrome: RustKit (default; ...)\nINFO hiwave::console: net start GET http://127.0.0.1:5000/index.html status=- issued nav=1\n"
    assert run.unobserved("hiwave", seen, runner) == []
    assert len(run.unobserved("hiwave", seen.replace("RustKit (default; ...)", "WebKit (HIWAVE_RUSTKIT_CHROME=0)"), runner)) == 1
    assert len(run.unobserved("hiwave", "INFO hiwave: Chrome: RustKit (default)\n", runner)) == 1  # the request log is off
    assert len(run.unobserved("hiwave", "", runner)) == 2
    assert run.unobserved("hiwave-headless", seen.split("\n", 1)[1], runner) == []  # no chrome to ask about
    # End to end: an engine whose request log says nothing is not an offline run.
    code, rec = run_with_fake_engine(monkeypatch, tmp_path / "quiet", "", request_log=False)
    assert code == 1 and rec["status"] == "invalid" and rec["score"] is None and "unobserved" in rec["off_machine"], rec


def test_the_turn_budget_is_raised_only_when_its_variable_is_set(monkeypatch, tmp_path):
    # A stress run may raise the engine's live-turn watchdog so a long computation (MotionMark's end-of-test
    # analysis) finishes and the next blocker shows. Without the variable the engine keeps its own default.
    monkeypatch.delenv(run.TURN_BUDGET_ENV, raising=False)
    assert run.turn_budget_ms() is None
    argv = run.headless_argv(pathlib.Path("pc"), "http://127.0.0.1:1/", tmp_path, 60, None)
    assert "--live-turn-budget-ms" not in argv and "--interrupt-scripts" in argv
    monkeypatch.setenv(run.TURN_BUDGET_ENV, "600000")
    assert run.turn_budget_ms() == 600000
    argv = run.headless_argv(pathlib.Path("pc"), "http://127.0.0.1:1/", tmp_path, 60, 600000)
    assert argv[argv.index("--live-turn-budget-ms") + 1] == "600000"
    for bad in ("", "abc", "0", "-5"):
        monkeypatch.setenv(run.TURN_BUDGET_ENV, bad)
        try:
            run.turn_budget_ms()
            raise AssertionError("accepted %r" % bad)
        except SystemExit:
            pass


def test_a_raised_budget_run_never_enters_a_default_comparison(tmp_path):
    # Same suite, engine and label: a raised-budget run is still not folded into the default median.
    record(tmp_path, "motionmark", "base", 100.0, 0)
    record(tmp_path, "motionmark", "cand", 100.0, 0)
    record(tmp_path, "motionmark", "cand", 5000.0, 1, budget=60000)
    record(tmp_path, "motionmark", "cand", 5000.0, 2, budget=60000)
    code, rows = compare(tmp_path)
    assert code == 0 and rows["motionmark"]["cand"]["n"] == 1, rows
    # On its own, with --raised-budget, it is reported, and only it.
    p = subprocess.run([sys.executable, str(HERE / "report.py"), "--raised-budget", "--json", str(tmp_path)],
                       capture_output=True, text=True)
    rows = json.loads(p.stdout)
    assert [(r["label"], r["n"], r["median"]) for r in rows] == [("cand", 2, 5000.0)], rows


def test_the_turn_budget_takes_plain_digits_up_to_an_hour(monkeypatch):
    for bad in (" 60000 ", "+5", "1_000", "６０", "3600001", "99999999999"):
        monkeypatch.setenv(run.TURN_BUDGET_ENV, bad)
        try:
            run.turn_budget_ms()
            raise AssertionError("accepted %r" % bad)
        except SystemExit:
            pass
    monkeypatch.setenv(run.TURN_BUDGET_ENV, "3600000")
    assert run.turn_budget_ms() == run.TURN_BUDGET_MAX_MS == 3600000


def test_a_raised_budget_is_refused_for_chrome_only(monkeypatch, tmp_path):
    # Chrome has no such watchdog. The app reads the variable (it is what run.py starts it with), and
    # the headless engine takes it as an argument: neither is refused here (this run stops later, at
    # its missing binary).
    monkeypatch.setenv(run.TURN_BUDGET_ENV, "60000")
    for engine, said in (("chrome", "HiWave engines only"), ("hiwave", "no binary at"), ("hiwave-headless", "no binary at")):
        monkeypatch.setattr(sys, "argv", ["run.py", "--suite", "motionmark", "--engine", engine,
                                          "--binary", str(tmp_path / "nothing"), "--out", str(tmp_path / "out")])
        try:
            run.main()
            raise AssertionError("ran %s with no binary" % engine)
        except SystemExit as e:
            assert said in str(e.code), (engine, e.code)
    assert not (tmp_path / "out").exists()


def test_the_budget_is_not_taken_through_env_and_the_app_takes_at_most_five_minutes(monkeypatch, tmp_path):
    # Through --env the variable would reach the app with no flag in the record and no raised-budget line:
    # a score nobody could tell from a default one. And the app ignores more than five minutes.
    monkeypatch.delenv(run.TURN_BUDGET_ENV, raising=False)
    base = ["run.py", "--suite", "motionmark", "--binary", str(tmp_path / "nothing"), "--out", str(tmp_path / "out")]
    for engine in ("hiwave", "hiwave-headless"):
        monkeypatch.setattr(sys, "argv", base + ["--engine", engine, "--env", run.TURN_BUDGET_ENV + "=60000"])
        try:
            run.main()
            raise AssertionError("took the budget through --env for %s" % engine)
        except SystemExit as e:
            assert "is not a --env variable" in str(e.code), e.code
    monkeypatch.setenv(run.TURN_BUDGET_ENV, str(run.APP_TURN_BUDGET_MAX_MS + 1))
    monkeypatch.setattr(sys, "argv", base + ["--engine", "hiwave"])
    try:
        run.main()
        raise AssertionError("started the app with more than it honours")
    except SystemExit as e:
        assert "at most 300000 ms for the app" in str(e.code), e.code
    # The app honours the variable only beside the console log switch, which every app run has.
    assert run.NET_LOG_ENV["HIWAVE_CONSOLE_LOG"] == "1"
    assert not (tmp_path / "out").exists()


def test_the_app_is_started_with_the_runs_budget_and_never_with_the_shells(monkeypatch):
    # A raised run: the app's environment has the run's number (the one after the clamp), and the record's
    # flags say so. A default run: nothing is passed, and a value the shell had is taken out of what the
    # app inherits, so a score without the flag never ran under one.
    monkeypatch.setenv(run.TURN_BUDGET_ENV, "999999")
    assert run.app_turn_budget_env(60000) == {run.TURN_BUDGET_ENV: "60000"}
    assert os.environ[run.TURN_BUDGET_ENV] == "999999"
    assert run.app_turn_budget_env(None) == {}
    assert run.TURN_BUDGET_ENV not in os.environ
    shell = {run.TURN_BUDGET_ENV: "999999", "HIWAVE_FROM_THE_SHELL": "yes"}
    assert run.engine_flags("hiwave", {}, inherited=shell, turn_budget=60000)[run.TURN_BUDGET_ENV] == "60000"
    assert run.TURN_BUDGET_ENV not in run.engine_flags("hiwave", {}, inherited=shell)
    # The headless engine takes an argument, not the variable; Chrome reads nothing.
    assert run.TURN_BUDGET_ENV not in run.engine_flags("hiwave-headless", {}, inherited=shell, turn_budget=60000)
    assert run.engine_flags("chrome", {}, inherited=shell, turn_budget=60000) == {}


# The one failure FLK8 saw (once in 26, a loaded Mac): the run came back invalid after its full wall
# limit with no request-log line, so the fake engine never reached the runner. Its cause is not known.
NEVER_REACHED = "no request-log line for the runner page"


def outcome(code, rec):
    """What a fake-engine run did, for an assertion message: status, exit codes, wall time, and the
    engine's own log (its first line says when it started)."""
    return "run.py exit %r, status %r, engine exit %r, %s s after launch, off_machine %r; engine log:\n%s" % (
        code, rec.get("status"), rec.get("exit_code"), rec.get("wall_s"), rec.get("off_machine"),
        rec.get("log_tail", "(no tail: the run was ok)"))


def run_ok_with_fake_engine(monkeypatch, tmp_path, capsys):
    """A fake-engine run that must succeed. Retried once, and only on the FLK8 signature (the engine
    never reached the runner); any other failure, or that one twice, fails with the run's outcome.
    Returns the run and what it printed."""
    code, rec = run_with_fake_engine(monkeypatch, tmp_path / "first", "")
    printed = capsys.readouterr()
    if code != 0 and NEVER_REACHED in json.dumps(rec.get("off_machine")):
        print("FLK8 signature, retrying once:", outcome(code, rec), file=sys.stderr)
        code, rec = run_with_fake_engine(monkeypatch, tmp_path / "retry", "")
        printed = capsys.readouterr()
    assert code == 0 and rec.get("status") == "ok", outcome(code, rec)
    return rec, printed


def test_a_raised_budget_run_says_so_and_records_it(monkeypatch, tmp_path, capsys):
    # The fake engine is started the way the real one is (sandbox-exec on the Mac).
    monkeypatch.setenv(run.TURN_BUDGET_ENV, "60000")
    rec, cap = run_ok_with_fake_engine(monkeypatch, tmp_path / "raised", capsys)
    # The run's wall limit is 20 s here, so the budget is clamped to it.
    assert rec["live_turn_budget_ms"] == 20000, rec
    assert "TURN BUDGET RAISED" in cap.err and "clamped to the run's wall limit, 20000 ms" in cap.err, cap.err
    assert "TURN BUDGET RAISED to 20000 ms" in cap.out, cap.out
    monkeypatch.delenv(run.TURN_BUDGET_ENV)
    rec, cap = run_ok_with_fake_engine(monkeypatch, tmp_path / "default", capsys)
    assert rec["live_turn_budget_ms"] is None, rec
    assert "TURN BUDGET RAISED" not in cap.err + cap.out, cap


def test_motionmark_is_given_the_frames_flag_and_the_other_suites_are_not():
    # MotionMark's runner loads each test in an iframe and waits for its load event; the engine makes
    # no request for a frame unless HIWAVE_NESTED_CONTEXTS=1, so without it the suite never starts.
    assert run.suite_env("motionmark", {}) == {"HIWAVE_NESTED_CONTEXTS": "1"}
    for other in ("speedometer", "jetstream", "control"):
        assert run.suite_env(other, {}) == {}, other
    # The caller's --env is applied over the suite's (a run that looks at the suite without frames) ...
    assert run.suite_env("motionmark", {"HIWAVE_NESTED_CONTEXTS": "0", "KEEP": "me"}) == \
        {"HIWAVE_NESTED_CONTEXTS": "0", "KEEP": "me"}
    # ... and the offline checks' settings still go on last.
    env, _ = run.engine_env(run.suite_env("motionmark", {"HIWAVE_RUSTKIT_CHROME": "0"}), inherited={})
    assert env["HIWAVE_NESTED_CONTEXTS"] == "1" and env["HIWAVE_RUSTKIT_CHROME"] == "1", env


def test_the_engine_flags_are_what_the_engine_process_is_started_with():
    shell = {"HIWAVE_CONTENT_THREAD": "0", "HIWAVE_FROM_THE_SHELL": "yes", "HOME": "/x", run.TURN_BUDGET_ENV: "60000"}
    asked = run.suite_env("motionmark", {"HIWAVE_ASKED": "1", "RUST_BACKTRACE": "1"})
    # Every HIWAVE_ variable in the engine's environment, the shell's included; not the dropped one, not
    # the shell's own turn-budget variable (only a run that raised the budget passes one, to the app), nothing else.
    assert run.engine_flags("hiwave-headless", asked, inherited=shell) == {
        "HIWAVE_ASKED": "1", "HIWAVE_CONSOLE_LOG": "1", "HIWAVE_FROM_THE_SHELL": "yes",
        "HIWAVE_NESTED_CONTEXTS": "1", "HIWAVE_RUSTKIT_CHROME": "1"}
    # The app is also started with its test hooks off.
    assert run.engine_flags("hiwave", asked, inherited={})["HIWAVE_TEST_HOOKS"] == "0"
    # Chrome reads none of them.
    assert run.engine_flags("chrome", asked, inherited=shell) == {}


FLAG_ECHO = 'import os; print("fake engine sees nested=" + os.environ.get("HIWAVE_NESTED_CONTEXTS", "unset"), flush=True)'


def test_a_motionmark_run_starts_the_engine_with_the_frames_flag_and_records_it(monkeypatch, tmp_path):
    monkeypatch.delenv("HIWAVE_NESTED_CONTEXTS", raising=False)

    def engine_saw(tmp):
        log = next((tmp / "out").glob("*.work/app.log")).read_text()
        return [line for line in log.splitlines() if line.startswith("fake engine sees")]

    # motionmark: on, in the engine's environment and in the record.
    code, rec = run_with_fake_engine(monkeypatch, tmp_path / "mm", FLAG_ECHO, suite_name="motionmark", more_argv=("--keep",))
    assert code == 0 and rec["status"] == "ok", outcome(code, rec)
    assert rec["engine_flags"]["HIWAVE_NESTED_CONTEXTS"] == "1", rec["engine_flags"]
    assert engine_saw(tmp_path / "mm") == ["fake engine sees nested=1"]
    # another suite: not set, and the record says so by not listing it.
    code, rec = run_with_fake_engine(monkeypatch, tmp_path / "sp", FLAG_ECHO, more_argv=("--keep",))
    assert code == 0 and rec["status"] == "ok", outcome(code, rec)
    assert "HIWAVE_NESTED_CONTEXTS" not in rec["engine_flags"] and rec["engine_flags"]["HIWAVE_RUSTKIT_CHROME"] == "1", rec
    assert engine_saw(tmp_path / "sp") == ["fake engine sees nested=unset"]
    # motionmark with the flag switched off by the caller: off, and recorded as off.
    code, rec = run_with_fake_engine(monkeypatch, tmp_path / "off", FLAG_ECHO, suite_name="motionmark",
                                     more_argv=("--keep", "--env", "HIWAVE_NESTED_CONTEXTS=0"))
    assert code == 0 and rec["status"] == "ok", outcome(code, rec)
    assert rec["engine_flags"]["HIWAVE_NESTED_CONTEXTS"] == "0", rec["engine_flags"]
    assert engine_saw(tmp_path / "off") == ["fake engine sees nested=0"]


def test_the_engines_own_uncaught_errors_are_read_from_its_log(tmp_path):
    log = tmp_path / "app.log"
    log.write_text("\x1b[2m2026-11-16T01:21:24.9Z\x1b[0m  INFO hiwave::console: uncaught TypeError: cannot convert at script#9:94:31\n"
                   "    at script#9:94:31\n"
                   "2026-11-16T01:21:25.0Z  INFO hiwave::console: console log inline#1:2:3 uncaught is only a word here\n"
                   "2026-11-16T01:21:25.1Z  INFO hiwave::console: unhandled rejection Error: boom at script#2:1:1\n")
    assert run.log_uncaught(log) == ["uncaught TypeError: cannot convert at script#9:94:31",
                                     "unhandled rejection Error: boom at script#2:1:1"]
    assert run.log_uncaught(tmp_path / "missing.log") == []


def test_an_error_the_page_cancelled_is_among_the_engines_uncaught_errors_with_its_mark(tmp_path):
    # The engine writes an error the page cancelled (window.onerror returned true, or a listener called
    # preventDefault) with a mark after the word the reader matches on: a page cannot keep it out of the record.
    log = tmp_path / "app.log"
    log.write_text("2026-11-16T17:00:00.0Z  INFO hiwave::console: uncaught (cancelled by the page) Error: boom at inline#2:1:7\n"
                   "2026-11-16T17:00:00.1Z  INFO hiwave::console: uncaught RangeError: left alone at inline#3:1:7\n")
    assert run.log_uncaught(log) == ["uncaught (cancelled by the page) Error: boom at inline#2:1:7",
                                     "uncaught RangeError: left alone at inline#3:1:7"]


def test_a_message_with_an_escaped_line_break_is_one_error_to_the_reader(tmp_path):
    # The engine writes control characters in page text escaped, so text shaped like a log line inside a
    # message stays inside its own line.
    log = tmp_path / "app.log"
    log.write_text("2026-11-16T17:00:00.0Z  INFO hiwave::console: uncaught Error: first\\n2026-11-16T00:00:00.0Z  INFO "
                   "hiwave::console: uncaught Error: second at inline#9:1:1 at inline#1:1:1\n    at inline#1:1:1\n"
                   "2026-11-16T17:00:00.1Z  INFO hiwave::console: unhandled rejection (cancelled by the page) Error: boom at inline#2:1:1\n")
    found = run.log_uncaught(log)
    assert len(found) == 2 and found[0].startswith("uncaught Error: first\\n2026-11-16T00:00:00.0Z") and "second" in found[0]
    assert found[1] == "unhandled rejection (cancelled by the page) Error: boom at inline#2:1:1"


def test_the_stress_reader_matches_the_engines_line_prefix(tmp_path):
    # A record is a physical line that starts with the engine's own prefix: a timestamp, a level, and the
    # console target right after them. The same words anywhere else on a line (in the middle or at the start
    # of another target's message, after other text) are not a record.
    log = tmp_path / "app.log"
    stamp = "2026-11-16T17:00:00.000000Z"
    log.write_text(
        stamp + "  INFO hiwave::console: uncaught Error: the real one at inline#1:1:1\n"
        + stamp + "  WARN rustkit_engine: Web font not loaded family=x " + stamp + "  INFO hiwave::console: uncaught Error: in the middle at inline#9:1:1\n"
        + stamp + "  INFO rustkit_engine: hiwave::console: uncaught Error: at the start of a message at inline#9:1:1\n"
        + stamp + "  INFO rustkit_net::policy: " + stamp + "  INFO hiwave::console: unhandled rejection Error: after a target at inline#9:1:1\n"
        + "hiwave::console: uncaught Error: no prefix at inline#9:1:1\n"
        + "  INFO hiwave::console: uncaught Error: no timestamp at inline#9:1:1\n"
        + stamp + " hiwave::console: uncaught Error: no level at inline#9:1:1\n"
        + "\x1b[2m" + stamp + "\x1b[0m \x1b[32m INFO\x1b[0m \x1b[2mhiwave::console\x1b[0m\x1b[2m:\x1b[0m unhandled rejection Error: coloured, as the app writes it at inline#2:1:1\n")
    assert run.log_uncaught(log) == ["uncaught Error: the real one at inline#1:1:1",
                                     "unhandled rejection Error: coloured, as the app writes it at inline#2:1:1"]


def test_a_finished_motionmark_tests_frames_are_kept_by_test_name():
    progress = [{"event": "test", "test": "Multiply"}, {"event": "test-done", "test": "Multiply", "frames": {"samples": 3}},
                {"event": "test-done", "test": "Leaves", "frames": None}, {"event": "start"}]
    assert run.frames_by_test(progress) == {"Multiply": {"samples": 3}}
