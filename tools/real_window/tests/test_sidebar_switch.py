"""The app the driver starts has its sidebar state set by HIWAVE_TEST_SIDEBAR,
not by the profile's settings.json: `open` for the real-window checks (their
clicks sit right of the sidebar), and a launcher's own env wins (the stress
harness and the latency session pass `closed`). No window: /usr/bin/env stands
in for the app binary and prints the environment it was started with.
"""

import pathlib
import sys
import time

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))

import driver as rw  # noqa: E402


def started_env(tmp_path, env=None):
    log = tmp_path / "app.log"
    app = rw.App(pathlib.Path("/usr/bin/env"), tmp_path, log, env).start()
    end = time.time() + 10
    while app.proc.poll() is None and time.time() < end:
        time.sleep(0.05)
    app.log.close()
    return dict(line.split("=", 1) for line in log.read_text().splitlines() if "=" in line)


def test_the_real_window_app_starts_with_its_sidebar_pinned_open(tmp_path, monkeypatch):
    monkeypatch.setenv("HIWAVE_TEST_SIDEBAR", "closed")  # the shell's value does not decide
    assert started_env(tmp_path)["HIWAVE_TEST_SIDEBAR"] == "open"


def test_a_launchers_env_closes_it(tmp_path):
    assert started_env(tmp_path, {"HIWAVE_TEST_SIDEBAR": "closed"})["HIWAVE_TEST_SIDEBAR"] == "closed"
