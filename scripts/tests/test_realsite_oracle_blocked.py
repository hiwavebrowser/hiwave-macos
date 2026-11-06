#!/usr/bin/env python3
"""T19: the real-site board must not score RustKit against Chrome's own error page.

Ported from atlas/trench-realsite (A1 + A2): when the Chrome oracle lands on a
403/challenge page or a chrome-error:// page, the site is `oracle_blocked`:
READABLE/LOOKS RIGHT are oracle failures (not RustKit fails), the summary names
it, and `n/scorable` leaves it out. The oracle runs headed, off-screen, by
default (A1); REALSITE_ORACLE_HEADLESS=1 / --oracle-headless restores headless.

No browser and no network: the Chrome and RustKit captures are stubbed.
"""

import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

REPO = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(REPO / "scripts"))

import realsite_board  # noqa: E402

PPM_W, PPM_H = 4, 4


def write_ppm(path, striped):
    px = bytearray()
    for y in range(PPM_H):
        for x in range(PPM_W):
            px += bytes((0, 0, 0)) if (striped and x == 0) else bytes((255, 255, 255))
    Path(path).write_bytes(b"P6\n%d %d\n255\n" % (PPM_W, PPM_H) + bytes(px))


def fake_chrome(result):
    def capture(url, png, text_json, width, height, env):
        Path(png).write_bytes(b"png")
        Path(text_json).write_text(json.dumps({"url": url, "text": "access denied"}))
        return dict(result)
    return capture


def fake_rustkit(cmd, timeout, env=None, **_kw):
    frame = Path(cmd[cmd.index("--dump-frame") + 1])
    write_ppm(frame, striped=True)
    Path(cmd[cmd.index("--dump-display-list") + 1]).write_text(
        json.dumps({"commands": [{"op": "text", "text": "access denied", "x": 1, "y": 2}]}))
    return {"status": "ok"}, None, 0


def score(chrome_result):
    out = Path(tempfile.mkdtemp(prefix="t19_"))
    try:
        with mock.patch.object(realsite_board, "chrome_capture", fake_chrome(chrome_result)), \
             mock.patch.object(realsite_board, "run_json", fake_rustkit), \
             mock.patch.object(realsite_board, "probe_access",
                               lambda url: {"status": 200, "vendor": None, "blocked": False}), \
             mock.patch.object(realsite_board, "pixel_diff",
                               lambda a, b, d, env: {"diffPercent": 1.0}), \
             mock.patch.object(realsite_board, "non_background_fraction", lambda p: 0.25):
            return realsite_board.score_site({"id": "x", "url": "https://x.test/"},
                                             Path("/bin/true"), out, 1280, 800, {})
    finally:
        shutil.rmtree(out, ignore_errors=True)


OK = {"status": "ok", "final_url": "https://x.test/", "http_status": 200,
      "browser_version": realsite_board.PINNED_CHROME_VERSION}


class OracleBlocked(unittest.TestCase):

    def test_http_403_oracle_is_oracle_blocked_not_a_pass(self):
        r = score(dict(OK, http_status=403))
        self.assertIsNone(r["oracle_capture"])
        self.assertEqual(r["chrome"][0].get("oracle_blocked"), "top-level document HTTP 403")
        for check in ("readable", "looks_right"):
            self.assertFalse(r[check]["pass"], check)
            self.assertTrue(r[check].get("oracle_failed"), check)
        self.assertIn("HTTP 403", r["readable"]["why"])
        # LOADS is RustKit's own check and is not touched by the oracle.
        self.assertTrue(r["loads"]["pass"])

    def test_chrome_error_page_is_oracle_blocked(self):
        r = score(dict(OK, final_url="chrome-error://chromewebdata/", http_status=None,
                       nav_error="net::ERR_HTTP_RESPONSE_CODE_FAILURE"))
        self.assertIsNone(r["oracle_capture"])
        self.assertIn("chrome error page", r["chrome"][0]["oracle_blocked"])

    def test_good_oracle_still_scores(self):
        r = score(OK)
        self.assertEqual(r["oracle_capture"], "a")
        self.assertNotIn("oracle_blocked", r["chrome"][0])
        self.assertEqual(r["points"], 3)

    def test_summary_names_oracle_blocked_and_leaves_it_out_of_scorable(self):
        blocked = score(dict(OK, http_status=403))
        good = dict(score(OK), id="g")
        s = realsite_board.summarize([blocked, good], "ts", 1280, 800, ["x", "g"])
        self.assertEqual(s["oracle_blocked"], {"x": "top-level document HTTP 403"})
        self.assertEqual(s["points"], 4)          # /6: the plain board is unchanged
        self.assertEqual(s["max_points"], 6)
        self.assertEqual(s["scorable_sites"], 1)  # n/scorable drops x
        self.assertEqual(s["scorable_points"], 3)
        self.assertEqual(s["scorable_max"], 3)

    def test_one_blocked_capture_falls_back_to_the_other(self):
        caps = iter([dict(OK, http_status=403), OK])
        out = Path(tempfile.mkdtemp(prefix="t19_"))
        try:
            def capture(url, png, text_json, width, height, env):
                Path(png).write_bytes(b"png")
                Path(text_json).write_text(json.dumps({"url": url, "text": "access denied"}))
                return dict(next(caps))
            with mock.patch.object(realsite_board, "chrome_capture", capture), \
                 mock.patch.object(realsite_board, "run_json", fake_rustkit), \
                 mock.patch.object(realsite_board, "probe_access",
                                   lambda url: {"status": 200, "vendor": None, "blocked": False}), \
                 mock.patch.object(realsite_board, "pixel_diff",
                                   lambda a, b, d, env: {"diffPercent": 1.0}), \
                 mock.patch.object(realsite_board, "non_background_fraction", lambda p: 0.25):
                r = realsite_board.score_site({"id": "x", "url": "https://x.test/"},
                                              Path("/bin/true"), out, 1280, 800, {})
        finally:
            shutil.rmtree(out, ignore_errors=True)
        self.assertEqual(r["oracle_capture"], "b")
        # Self-noise needs two real oracles; one is an error page.
        self.assertNotIn("chrome_self_diff", r["looks_right"])
        s = realsite_board.summarize([r], "ts", 1280, 800, ["x"])
        self.assertEqual(s["oracle_blocked"], {})  # only when BOTH captures are blocked


NODE = shutil.which("node")
LAUNCH_MJS = REPO / "tools" / "parity_oracle" / "realsite_launch.mjs"


def launch_options(env_extra):
    code = ("import { oracleLaunchOptions } from %s;"
            "console.log(JSON.stringify(oracleLaunchOptions(1280, 800, process.env)));"
            % json.dumps(LAUNCH_MJS.as_uri()))
    env = {k: v for k, v in os.environ.items() if k != "REALSITE_ORACLE_HEADLESS"}
    env.pop("PARITY_CHROME_PATH", None)
    env.update(env_extra)
    p = subprocess.run([NODE, "--input-type=module", "-e", code], capture_output=True,
                       text=True, env=env, timeout=60)
    if p.returncode != 0:
        raise AssertionError("node failed: %s" % p.stderr.strip()[-400:])
    return json.loads(p.stdout.strip().splitlines()[-1])


@unittest.skipUnless(NODE, "node not on PATH")
class OracleHeadedA1(unittest.TestCase):

    def test_default_is_headed_and_off_screen(self):
        o = launch_options({})
        self.assertIs(o["headless"], False)
        self.assertIn("--window-position=-2400,0", o["args"])
        self.assertIn("--window-size=1280,800", o["args"])
        self.assertIn("--force-color-profile=srgb", o["args"])  # deterministic flags kept

    def test_env_restores_headless(self):
        o = launch_options({"REALSITE_ORACLE_HEADLESS": "1"})
        self.assertIs(o["headless"], True)
        self.assertFalse(any(a.startswith("--window-position") for a in o["args"]))

    def test_board_flag_sets_the_env(self):
        src = (REPO / "scripts" / "realsite_board.py").read_text(encoding="utf-8")
        self.assertIn("--oracle-headless", src)
        self.assertIn("REALSITE_ORACLE_HEADLESS", src)

    def test_board_capture_uses_the_shared_launch_and_records_status(self):
        src = (REPO / "tools" / "parity_oracle" / "realsite.mjs").read_text(encoding="utf-8")
        body = src[src.index("async function captureChrome"):src.index("async function runActionsChrome")]
        self.assertIn("oracleLaunchOptions(", body)
        self.assertIn("http_status", body)
        self.assertIn("isNavigationRequest()", body)


if __name__ == "__main__":
    unittest.main()
