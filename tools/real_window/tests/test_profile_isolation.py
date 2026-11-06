"""The driver's checks each start on the seeded profile, never on what an
earlier check in the same run left in it (CH3.A4b).

`a4_settings` unticks 'Sidebar open by default' and saves, so settings.json
says the sidebar starts collapsed; a later check on the same profile (a4_private,
e4_chrome_tab, b11_settings_save) then launched with no sidebar and clicked
where its button or a tab would have been. No app, no window: a fake App
records what the profile held when each check's app started.
"""

import json
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))

import driver as rw  # noqa: E402


class FakeApp:
    """Stands in for driver.App: the seed launch writes the first-run state;
    every launch records the profile's files as the app would have read them."""

    launches = []

    def __init__(self, binary, profile, log_path, env=None):
        self.binary, self.profile, self.log_path, self.env = binary, profile, log_path, env

    @property
    def state_file(self):
        return self.profile / "Library" / "Application Support" / "hiwave" / "workspace_state.json"

    def start(self):
        data = self.state_file.parent
        if not self.state_file.is_file():
            data.mkdir(parents=True, exist_ok=True)
            self.state_file.write_text(json.dumps({"tabs": [{"url": "hiwave://newtab"}]}))
            (data / "vault.db").write_text("first-run vault")
        FakeApp.launches.append({p.name: p.read_text() for p in data.iterdir() if p.is_file()})
        self.proc = type("P", (), {"pid": 4242})()
        return self

    def alive(self):
        return True

    def wait_log(self, pattern, timeout, start=0):
        return None

    def log_text(self):
        return ""

    def stop(self):
        pass


class FakeFixture:
    base = "http://127.0.0.1:1"

    def mark(self):
        return 0

    def wait(self, needle, timeout, since=0, count=1):
        return True


def driver_at(out, monkeypatch):
    monkeypatch.setattr(rw, "App", FakeApp)
    monkeypatch.setattr(rw.time, "sleep", lambda s: None)
    d = object.__new__(rw.Driver)
    d.binary, d.out, d.fixture, d.template, d.app, d.results = out / "hiwave", out, FakeFixture(), None, None, []
    d.can_input = False
    d.hw = lambda *argv: (0, {"w": 1280, "h": 832})
    return d


def test_a_check_does_not_see_what_an_earlier_check_saved(tmp_path, monkeypatch):
    FakeApp.launches = []
    d = driver_at(tmp_path, monkeypatch)

    assert d.open_page("a4_settings", "a3_page.html")
    data = d.app.state_file.parent
    # What a4_settings leaves behind: the flipped default saved, the vault set up.
    (data / "settings.json").write_text(json.dumps({"sidebar_default_open": False}))
    (data / "vault.db").write_text("a master password was set")
    d.close_page()

    assert d.open_page("a4_private", "a3_page.html")
    seen = FakeApp.launches[-1]
    assert "settings.json" not in seen, "a4_private started on a4_settings' saved settings"
    assert seen.get("vault.db") == "first-run vault", "a4_private started on a4_settings' vault"
    assert "a3_page.html" in seen["workspace_state.json"], "the check's page is still the restored tab"


def test_the_seed_launch_runs_once_per_run(tmp_path, monkeypatch):
    FakeApp.launches = []
    d = driver_at(tmp_path, monkeypatch)
    for check in ("h1", "h2", "h3"):
        assert d.open_page(check, "a3_page.html")
        d.close_page()
    # One seed launch plus one launch per check.
    assert len(FakeApp.launches) == 4
