"""`driver.py --record` screen-records the whole run (T26).

A fake `screencapture` stands in for the real one: it starts, waits for the
character `screencapture -v` stops on (or SIGINT), then writes the file, as
the real one finalises its .mov on stop. `hwdrive movie` is answered by the
test. No app, no window, no grant.
"""

import json
import os
import pathlib
import signal
import subprocess
import sys
import textwrap
import time

import pytest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))

import driver as rw  # noqa: E402

FAKE = textwrap.dedent("""\
    #!{python}
    import os, signal, sys, time
    out = sys.argv[-1]
    open(out + ".started", "w").write(" ".join(sys.argv[1:]))
    def finish(*_):
        open(out, "wb").write(b"MOV")
        os.remove(out + ".started")
        sys.exit(0)
    signal.signal(signal.SIGINT, finish)
    sys.stdin.buffer.read(1)
    finish()
""")


def fake_screencapture(tmp_path):
    exe = tmp_path / "screencapture"
    exe.write_text(FAKE.format(python=sys.executable))
    exe.chmod(0o755)
    return exe


def driver_at(tmp_path, monkeypatch, checks, *, can_capture=True, movie_seconds=None, label="batch33"):
    monkeypatch.setattr(rw.Driver, "SCREENCAPTURE", str(fake_screencapture(tmp_path)))
    d = object.__new__(rw.Driver)
    out = tmp_path / "out"
    out.mkdir()
    d.binary, d.out, d.template, d.app, d.results = tmp_path / "hiwave", out, None, None, []
    d.binary.write_bytes(b"not an app")
    d.fixture = type("F", (), {"requests": []})()
    d.pre, d.can_capture, d.can_input = {"screen_capture": can_capture, "screen_locked": False}, can_capture, False
    d.record_dir = tmp_path / "recordings"
    d.record_label = label
    d.record_max_s = 600
    d.close_page = lambda: None
    seen = {}

    def hw(*argv):
        if argv[0] == "movie":
            seen["movie"] = argv[1]
            ran = time.monotonic() - seen["start"]
            secs = ran if movie_seconds is None else movie_seconds
            return 0, {"seconds": secs, "playable": True, "video_tracks": 1}
        return 0, {}
    d.hw = hw
    for name, fn in checks.items():
        fn.__doc__ = fn.__doc__ or "a fake check."
        setattr(d, name, fn)
    seen["start"] = time.monotonic()
    return d, seen


def movs(d):
    return sorted(d.record_dir.glob("*.mov")) if d.record_dir.is_dir() else []


def record_rows(d):
    return [r for r in d.results if r[0] == "record"]


def test_a_recorded_run_leaves_one_playable_mov_named_by_batch_and_checks(tmp_path, monkeypatch):
    def h1():
        time.sleep(1.0)
    d, seen = driver_at(tmp_path, monkeypatch, {"h1": h1})
    code = d.run(["h1"])
    files = movs(d)
    assert len(files) == 1, files
    assert files[0].read_bytes() == b"MOV", "the recording was not stopped cleanly"
    assert "batch33" in files[0].name and files[0].name.endswith("-h1.mov"), files[0].name
    assert not list(d.record_dir.glob("*.started")), "screencapture still running after the run"
    assert seen["movie"] == str(files[0])
    rows = record_rows(d)
    assert rows and all(r[2] == "PASS" for r in rows), rows
    assert code == 0
    summary = json.loads((d.out / "result.json").read_text())
    assert summary["recording"]["path"] == str(files[0])


def test_the_full_check_set_is_named_all(tmp_path, monkeypatch):
    d, _ = driver_at(tmp_path, monkeypatch, {})
    assert d.recording_name(list(rw.Driver.CHECKS)).endswith("-batch33-all.mov")
    many = list(rw.Driver.CHECKS[:7])
    name = d.recording_name(many)
    assert name.endswith(".mov") and "7checks" in name and len(name) < 80, name


def test_an_aborted_run_still_stops_the_recording(tmp_path, monkeypatch):
    def h1():
        raise RuntimeError("check blew up")
    d, _ = driver_at(tmp_path, monkeypatch, {"h1": h1})
    with pytest.raises(RuntimeError):
        d.run(["h1"])
    files = movs(d)
    assert len(files) == 1 and files[0].read_bytes() == b"MOV"
    assert not list(d.record_dir.glob("*.started"))


def test_a_recording_whose_length_is_far_from_the_run_fails(tmp_path, monkeypatch):
    def h1():
        time.sleep(0.2)
    d, _ = driver_at(tmp_path, monkeypatch, {"h1": h1}, movie_seconds=60.0)
    assert d.run(["h1"]) == 1
    assert any(r[2] == "FAIL" and "length" in r[1] for r in record_rows(d)), record_rows(d)


def test_no_screen_recording_grant_is_not_run_and_starts_nothing(tmp_path, monkeypatch):
    def h1():
        pass
    d, _ = driver_at(tmp_path, monkeypatch, {"h1": h1}, can_capture=False)
    assert d.run(["h1"]) == 3
    assert movs(d) == []
    rows = record_rows(d)
    assert rows and rows[0][2] == "NOT RUN" and "Screen Recording" in rows[0][3], rows


def test_a_record_dir_inside_the_checkout_is_refused():
    repo = rw.HERE.parents[1]
    assert rw.record_dir_problem(repo / "tools" / "real_window" / "out")
    assert rw.record_dir_problem(repo)
    assert rw.record_dir_problem(pathlib.Path.home() / "Library" / "Logs" / "hiwave" / "recordings") is None


def test_sigterm_mid_run_stops_the_recording(tmp_path):
    """The batch runner kills the driver: the .mov is still finalised."""
    harness = tmp_path / "harness.py"
    harness.write_text(textwrap.dedent("""\
        import pathlib, sys, time
        sys.path.insert(0, %r)
        sys.path.insert(0, %r)
        import driver as rw
        import test_record as t
        class MP:
            def setattr(self, obj, name, value):
                setattr(obj, name, value)
        def h1():
            open(%r, "w").write("in check")
            time.sleep(60)
        d, _ = t.driver_at(pathlib.Path(%r), MP(), {"h1": h1})
        rw.install_stop_signals()
        d.run(["h1"])
    """) % (str(rw.HERE), str(pathlib.Path(__file__).parent), str(tmp_path / "in_check"), str(tmp_path / "w")))
    (tmp_path / "w").mkdir()
    p = subprocess.Popen([sys.executable, str(harness)], cwd=str(tmp_path))
    end = time.time() + 20
    while time.time() < end and not (tmp_path / "in_check").exists():
        time.sleep(0.1)
    assert (tmp_path / "in_check").exists(), "harness never reached the check"
    os.kill(p.pid, signal.SIGTERM)
    p.wait(timeout=30)
    rec = tmp_path / "w" / "recordings"
    files = sorted(rec.glob("*.mov"))
    assert len(files) == 1 and files[0].read_bytes() == b"MOV", list(rec.iterdir())
    assert not list(rec.glob("*.started"))
