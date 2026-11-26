#!/usr/bin/env python3
"""Tests for the replay server, the trace diff, and the Chrome trace helpers."""

import json
import os
import socket
import ssl
import subprocess
import sys
import tempfile
import threading
import time
import unittest
import urllib.error
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

from diff_trace import first_divergence  # noqa: E402
from normalize import normalize_url  # noqa: E402
from server import HarArchive, serve  # noqa: E402

HAR = HERE / "fixtures" / "synthetic.har"
SECRET = "secret-cookie-value"


def request(port, url, headers=None):
    opener = urllib.request.Request(url, headers=headers or {}, method="GET")
    with urllib.request.urlopen(opener, timeout=5) as response:
        return response.status, dict(response.headers), response.read()


class NormalizeTests(unittest.TestCase):
    def test_sorts_query_and_drops_fragment_and_default_port(self):
        self.assertEqual(
            normalize_url("HTTPS://Example.TEST:443/search?b=2&a=1#frag"),
            "https://example.test/search?a=1&b=2",
        )
        self.assertEqual(normalize_url("http://example.test:8080/a"), "http://example.test:8080/a")
        self.assertEqual(normalize_url("https://example.test/a/"), "https://example.test/a/")


class ServerTests(unittest.TestCase):
    def setUp(self):
        self.miss = Path(tempfile.mkdtemp()) / "miss.log"
        self.server = serve(HAR, miss_log=self.miss, time_scale=1.0, tls=True)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.port = self.server.server_address[1]

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()

    def hiwave(self, original, headers=None):
        from urllib.parse import urlparse
        parsed = urlparse(original)
        path = parsed.path or "/"
        if parsed.query:
            path = f"{path}?{parsed.query}"
        url = f"http://127.0.0.1:{self.port}{path}"
        sent = {"X-Original-Url": original, "Host": parsed.netloc}
        if headers:
            sent.update(headers)
        return request(self.port, url, sent)

    def test_headers_query_nth_and_miss_log(self):
        status, headers, body = self.hiwave("https://example.test/")
        self.assertEqual(status, 200)
        self.assertIn(b"app.js", body)
        self.assertEqual(headers["X-Replay"], "page")
        self.assertEqual(int(headers["Content-Length"]), len(body))
        self.assertNotIn("Transfer-Encoding", headers)

        status, _, body = self.hiwave("https://example.test/search?a=1&b=2")
        self.assertEqual(body, b"found")

        status, headers, body = self.hiwave("https://example.test/app.js")
        self.assertEqual(body, b"window.__replay=1;")
        self.assertEqual(headers["Set-Cookie"], "synthetic=1")

        self.assertEqual(self.hiwave("https://example.test/nth")[2], b"first")
        self.assertEqual(self.hiwave("https://example.test/nth")[2], b"second")
        self.assertEqual(self.hiwave("https://example.test/nth")[2], b"second")

        with self.assertRaises(urllib.error.HTTPError) as caught:
            self.hiwave("https://example.test/missing", {"Cookie": SECRET})
        self.assertEqual(caught.exception.code, 404)
        logged = self.miss.read_text(encoding="utf-8")
        self.assertIn("MISS GET https://example.test/missing", logged)
        self.assertNotIn(SECRET, logged)

    def test_time_scale_zero_skips_the_recorded_wait(self):
        self.server.time_scale = 0
        started = time.monotonic()
        body = self.hiwave("https://example.test/slow")[2]
        self.assertEqual(body, b"slow")
        self.assertLess(time.monotonic() - started, 0.05)

    def test_time_scale_honours_recorded_time(self):
        self.server.time_scale = 1
        started = time.monotonic()
        self.hiwave("https://example.test/slow")
        self.assertGreaterEqual(time.monotonic() - started, 0.06)

    def test_connect_serves_the_same_entry(self):
        if self.server.tls is None:
            self.skipTest("openssl certificate was not created")
        sock = socket.create_connection(("127.0.0.1", self.port), timeout=5)
        sock.sendall(b"CONNECT example.test:443 HTTP/1.1\r\nHost: example.test:443\r\n\r\n")
        head = b""
        while b"\r\n\r\n" not in head:
            head += sock.recv(1024)
        self.assertIn(b"200", head.split(b"\r\n", 1)[0])
        tls = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
        tls.check_hostname = False
        tls.verify_mode = ssl.CERT_NONE
        wrapped = tls.wrap_socket(sock, server_hostname="example.test")
        wrapped.sendall(b"GET /app.js HTTP/1.1\r\nHost: example.test\r\nConnection: close\r\n\r\n")
        data = b""
        while True:
            chunk = wrapped.recv(4096)
            if not chunk:
                break
            data += chunk
        self.assertIn(b"200", data.split(b"\r\n", 1)[0])
        self.assertIn(b"window.__replay=1;", data)
        wrapped.close()


class DiffTests(unittest.TestCase):
    def trace(self, **overrides):
        base = {
            "browser": "hiwave",
            "url": "https://example.test/",
            "ready_state": [
                {"state": "loading", "t_ms": 0},
                {"state": "interactive", "t_ms": 5},
                {"state": "complete", "t_ms": 9},
            ],
            "dcl_ms": 5,
            "load_ms": 9,
            "scripts": [
                {"url": "https://example.test/app.js?b=1&a=2", "order": 0, "event": "execute", "t_ms": 1},
                {"url": "inline#2", "order": 1, "event": "execute", "t_ms": 2},
            ],
            "console": [{"level": "error", "text": "synthetic-error", "t_ms": 3}],
            "mutations": [{"t_ms": 4, "type": "add", "nodes": 61, "target": "body"}],
            "max_js_stack": 4,
        }
        base.update(overrides)
        return base

    def test_ignores_timing_query_order_and_a_lone_load(self):
        left = self.trace()
        right = self.trace(
            browser="chrome",
            scripts=[
                {"url": "https://Example.TEST/app.js?a=2&b=1", "order": 0, "event": "execute", "t_ms": 40},
                {"url": "https://Example.TEST/app.js?a=2&b=1", "order": 0, "event": "load", "t_ms": 41},
                {"url": "inline#2", "order": 1, "event": "execute", "t_ms": 50},
            ],
            max_js_stack=6,
        )
        self.assertIsNone(first_divergence(left, right))

    def test_reports_the_first_script_error(self):
        right = self.trace(
            scripts=[
                {"url": "https://example.test/app.js?b=1&a=2", "order": 0, "event": "error", "t_ms": 1},
            ]
        )
        found = first_divergence(self.trace(), right)
        self.assertIn("scripts[0]", found)
        self.assertIn("execute", found)
        self.assertIn("error", found)

    def test_reports_a_mutation_after_scripts_agree(self):
        right = self.trace(mutations=[])
        found = first_divergence(self.trace(), right)
        self.assertIn("mutations[0]", found)

    def test_render_compares_paint_and_ignores_the_box_rect(self):
        painted = {
            "t_ms": 1,
            "when": "quiet",
            "display_list_items_in_main": 4,
            "main": {
                "tag": "main",
                "id": "main-content",
                "class": "",
                "custom_element": "uncustomized",
                "hidden": False,
                "shadow": None,
                "style": {"display": "block", "visibility": "visible", "opacity": 1, "content-visibility": "visible", "position": "static", "height": "10px", "overflow": "visible"},
                "box": {"x": 0, "y": 0, "width": 10, "height": 10},
                "in_display_list": True,
                "skip": None,
                "children": [{
                    "tag": "shreddit-post",
                    "id": "",
                    "class": "post",
                    "custom_element": "defined",
                    "hidden": False,
                    "shadow": None,
                    "style": {"display": "block", "visibility": "visible", "opacity": 1, "content-visibility": "visible", "position": "static", "height": "10px", "overflow": "visible"},
                    "box": {"x": 1, "y": 2, "width": 3, "height": 4},
                    "in_display_list": True,
                    "skip": None,
                    "children": [],
                }],
            },
        }
        other = json.loads(json.dumps(painted))
        other["main"]["children"][0]["custom_element"] = "custom"
        other["main"]["children"][0]["box"] = {"x": 9, "y": 9, "width": 9, "height": 9}
        other["t_ms"] = 99
        self.assertIsNone(first_divergence(self.trace(render=painted), self.trace(render=other)))
        hidden = json.loads(json.dumps(painted))
        hidden["main"]["children"][0]["in_display_list"] = False
        hidden["main"]["children"][0]["skip"] = "display:none"
        hidden["main"]["children"][0]["style"]["display"] = "none"
        found = first_divergence(self.trace(render=painted), self.trace(render=hidden))
        self.assertIn("render.main.children[0].display", found)

    def test_reports_a_wildly_different_stack_last(self):
        right = self.trace(max_js_stack=400)
        found = first_divergence(self.trace(), right)
        self.assertIn("max_js_stack", found)

    def test_cli_names_chrome_and_hiwave(self):
        with tempfile.TemporaryDirectory() as tmp:
            chrome = Path(tmp) / "chrome.json"
            hiwave = Path(tmp) / "hiwave.json"
            chrome.write_text(json.dumps(self.trace(browser="chrome", mutations=[])), encoding="utf-8")
            hiwave.write_text(json.dumps(self.trace()), encoding="utf-8")
            result = subprocess.run(
                [
                    sys.executable,
                    str(HERE / "diff_trace.py"),
                    "--chrome",
                    str(chrome),
                    "--hiwave",
                    str(hiwave),
                ],
                check=False,
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("chrome:", result.stdout)
        self.assertIn("hiwave:", result.stdout)
        self.assertNotIn("secret-cookie-value", result.stdout)


def _har_entry(url, body, time_ms=0):
    return {
        "time": time_ms,
        "request": {"method": "GET", "url": url, "headers": []},
        "response": {
            "status": 200,
            "statusText": "OK",
            "headers": [{"name": "Content-Type", "value": "application/javascript"}],
            "content": {"mimeType": "application/javascript", "text": body},
        },
    }


def _sml(chunk_id):
    return f'SML.dm("{chunk_id}",[],()=>{{}});'


class ConcatTests(unittest.TestCase):
    def archive(self, entries):
        return HarArchive({"log": {"entries": entries}})

    def test_splits_recorded_registrations_into_the_requested_order(self):
        # Two recorded concat responses. A later request asks for a different
        # mix and order. The bodies are SML.dm registrations split on the
        # marker Reddit inserts between chunks.
        recorded = self.archive([
            _har_entry(
                "https://www.redditstatic.com/js/concat?prefix=en-US/&chunks=aaa,bbb",
                _sml("aaa") + "\n/* @sml-chunk */\n" + _sml("bbb"),
            ),
            _har_entry(
                "https://www.redditstatic.com/js/concat?prefix=en-US/&chunks=ccc",
                _sml("ccc"),
            ),
        ])
        entry, notes = recorded.take(
            "GET",
            "https://www.redditstatic.com/js/concat?prefix=en-US/&chunks=ccc,aaa",
        )
        text = entry["response"]["content"]["text"]
        self.assertLess(text.index('SML.dm("ccc"'), text.index('SML.dm("aaa"'))
        self.assertNotIn('SML.dm("bbb"', text)
        self.assertEqual(notes, ["CONCAT split chunks=2"])
        self.assertNotIn(SECRET, "\n".join(notes))

    def test_cover_uses_the_smallest_set_when_the_body_will_not_split(self):
        # No module-factory boundary: each recorded body stays whole.
        recorded = self.archive([
            _har_entry(
                "https://www.redditstatic.com/js/concat?prefix=en-US/&chunks=aaa,bbb",
                "whole-ab",
            ),
            _har_entry(
                "https://www.redditstatic.com/js/concat?prefix=en-US/&chunks=aaa",
                "only-a",
            ),
            _har_entry(
                "https://www.redditstatic.com/js/concat?prefix=en-US/&chunks=bbb",
                "only-b",
            ),
        ])
        entry, notes = recorded.take(
            "GET",
            "https://www.redditstatic.com/js/concat?prefix=en-US/&chunks=bbb,aaa",
        )
        self.assertEqual(entry["response"]["content"]["text"], "whole-ab")
        self.assertEqual(notes, ["CONCAT cover bodies=1"])

    def test_assembled_time_is_the_max_source_time(self):
        recorded = self.archive([
            _har_entry(
                "https://www.redditstatic.com/js/concat?prefix=en-US/&chunks=aaa",
                _sml("aaa"),
                time_ms=100,
            ),
            _har_entry(
                "https://www.redditstatic.com/js/concat?prefix=en-US/&chunks=bbb",
                _sml("bbb"),
                time_ms=4000,
            ),
        ])
        entry, _notes = recorded.take(
            "GET",
            "https://www.redditstatic.com/js/concat?prefix=en-US/&chunks=aaa,bbb",
        )
        self.assertEqual(entry["time"], 4000)

    def test_logs_a_chunk_the_har_does_not_have(self):
        recorded = self.archive([
            _har_entry(
                "https://www.redditstatic.com/js/concat?prefix=en-US/&chunks=aaa",
                _sml("aaa"),
            ),
        ])
        entry, notes = recorded.take(
            "GET",
            "https://www.redditstatic.com/js/concat?prefix=en-US/&chunks=aaa,zzz",
        )
        self.assertIn('SML.dm("aaa"', entry["response"]["content"]["text"])
        self.assertIn("CONCAT cover bodies=1", notes)
        self.assertIn("CONCAT missing zzz", notes)
        self.assertNotIn(SECRET, "\n".join(notes))

    def test_an_exact_concat_url_stays_on_the_recorded_body(self):
        recorded = self.archive([
            _har_entry(
                "https://www.redditstatic.com/js/concat?prefix=en-US/&chunks=aaa,bbb",
                "recorded-as-is",
            ),
        ])
        entry, notes = recorded.take(
            "GET",
            "https://www.redditstatic.com/js/concat?prefix=en-US/&chunks=aaa,bbb",
        )
        self.assertEqual(entry["response"]["content"]["text"], "recorded-as-is")
        self.assertEqual(notes, [])


class ChromeHelperTests(unittest.TestCase):
    def test_node_self_test(self):
        result = subprocess.run(
            ["node", str(HERE / "chrome_trace.mjs"), "--self-test"],
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("ok", result.stdout)


@unittest.skipUnless(os.path.exists("/usr/bin/google-chrome-stable"), "chrome not installed")
class ChromeReplayTests(unittest.TestCase):
    def test_chrome_trace_of_the_synthetic_har(self):
        server = serve(HAR, time_scale=0, tls=True)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        port = server.server_address[1]
        try:
            with tempfile.TemporaryDirectory() as tmp:
                out = Path(tmp) / "chrome.json"
                result = subprocess.run(
                    [
                        "node",
                        str(HERE / "chrome_trace.mjs"),
                        "--replay",
                        f"http://127.0.0.1:{port}",
                        "--url",
                        "https://example.test/",
                        "--out",
                        str(out),
                    ],
                    check=False,
                    capture_output=True,
                    text=True,
                    timeout=60,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                trace = json.loads(out.read_text(encoding="utf-8"))
        finally:
            server.shutdown()
            server.server_close()
        self.assertEqual(trace["browser"], "chrome")
        states = [item["state"] for item in trace["ready_state"]]
        self.assertEqual(states, ["loading", "interactive", "complete"])
        events = [(item["url"], item["event"]) for item in trace["scripts"]]
        self.assertEqual(
            events,
            [
                ("https://example.test/app.js", "execute"),
                ("https://example.test/app.js", "load"),
                ("inline#2", "execute"),
            ],
        )
        self.assertTrue(any("synthetic-error" in item["text"] for item in trace["console"]))
        self.assertTrue(any(item["type"] == "add" and item["target"] == "BODY" and item["nodes"] > 50 for item in trace["mutations"]))
        self.assertIsInstance(trace["parser"], list)
        self.assertIsNone(trace["render"]["main"])
        self.assertNotIn(SECRET, json.dumps(trace))


if __name__ == "__main__":
    unittest.main()
