"""
test_chrome_board.py — guards for the chrome board runner (CH3.A1).

1. A case whose fixture is missing is refused before anything is captured.
   A board row rendered without its state would score an empty page and
   call the chrome green.
2. Chromium and RustKit run the SAME action list: both commands name one
   actions file and that file holds the state's actions, verbatim. Two
   action lists would compare two different states and call the delta an
   engine defect.

Run: python3 scripts/tests/test_chrome_board.py
"""
import json
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(REPO))
sys.path.insert(0, str(REPO / "scripts"))

BOARD_PATH = REPO / "tools" / "parity_oracle" / "chrome_fixtures" / "board.json"


class TestChromeBoard(unittest.TestCase):
    def test_board_matrix_names_pages_states_and_sizes(self):
        board = json.loads(BOARD_PATH.read_text(encoding="utf-8"))
        self.assertEqual(sorted(board["pages"]), ["chrome", "inspector", "report", "settings", "shelf"])
        self.assertEqual(board["sizes"], [[1280, 800], [1024, 700], [1600, 1000]])
        for page, spec in board["pages"].items():
            self.assertTrue((REPO / spec["html"]).exists(), f"{page}: html {spec['html']} missing")
            for state in spec["states"]:
                self.assertIn(state, board["states"], f"{page}: state {state} not defined")

    def test_a_case_whose_fixture_is_missing_is_refused_before_any_capture(self):
        from scripts import chrome_board

        board = json.loads(BOARD_PATH.read_text(encoding="utf-8"))
        board["pages"]["chrome"]["fixture"] = "does-not-exist.js"
        with tempfile.TemporaryDirectory() as tmp:
            bad = Path(tmp) / "board.json"
            bad.write_text(json.dumps(board), encoding="utf-8")
            with self.assertRaises(chrome_board.BoardError) as ctx:
                chrome_board.plan(chrome_board.load_board(bad), REPO, Path(tmp) / "work")
            self.assertIn("chrome", str(ctx.exception))
            self.assertIn("does-not-exist.js", str(ctx.exception))

            captured = []
            rc = chrome_board.main(
                ["--parity-capture", "/nonexistent/parity-capture", "--board", str(bad),
                 "--work", str(Path(tmp) / "work"), "--out", str(Path(tmp) / "out")],
                capture=lambda *a, **k: captured.append(a),
            )
            self.assertNotEqual(rc, 0, "a missing fixture is a refusal, not a green board")
            self.assertEqual(captured, [], "nothing is captured once a fixture is missing")
            self.assertFalse((Path(tmp) / "out" / "BOARD.md").exists(), "no board is written")

    def test_chromium_and_rustkit_run_the_same_action_list(self):
        from scripts import chrome_board

        board = chrome_board.load_board(BOARD_PATH)
        with tempfile.TemporaryDirectory() as tmp:
            rows = chrome_board.plan(board, REPO, Path(tmp) / "work")
            self.assertEqual(len(rows), 14 * 3, "page x state x size")
            seen_states = set()
            for row in rows:
                chromium = chrome_board.chromium_command(row)
                rustkit = chrome_board.rustkit_command(row, Path("/bin/parity-capture"))
                self.assertIn(str(row.actions_path), chromium)
                self.assertIn("--actions", rustkit)
                self.assertEqual(rustkit[rustkit.index("--actions") + 1], str(row.actions_path))
                self.assertIn("--run-inline-scripts", rustkit, "the page's own script runs in RustKit too")
                on_disk = json.loads(row.actions_path.read_text(encoding="utf-8"))
                self.assertEqual(on_disk, board["states"][row.state], f"{row.page}/{row.state}: actions differ")
                self.assertEqual([str(row.width), str(row.height)], [rustkit[rustkit.index("--width") + 1], rustkit[rustkit.index("--height") + 1]])
                self.assertIn(str(row.width), chromium)
                self.assertIn(str(row.height), chromium)
                seen_states.add(row.state)
            self.assertIn("hover_tab2", seen_states)
            self.assertIn("focus_mode", seen_states)

    def test_the_composed_page_carries_the_stub_before_and_the_fixture_after_the_page_script(self):
        from scripts import chrome_board

        board = chrome_board.load_board(BOARD_PATH)
        with tempfile.TemporaryDirectory() as tmp:
            rows = chrome_board.plan(board, REPO, Path(tmp) / "work")
            chrome_rows = [r for r in rows if r.page == "chrome"]
            html = chrome_rows[0].html_path.read_text(encoding="utf-8")
            stub = html.index("window.__hiwaveBoard = {")
            page_script = html.index("window.hiwaveChrome = {")
            fixture = html.index("chrome.renderTabs(")
            self.assertLess(stub, page_script, "the IPC stub precedes the page's script")
            self.assertLess(page_script, fixture, "the fixture follows the page's script")
            report = [r for r in rows if r.page == "report"][0].html_path.read_text(encoding="utf-8")
            self.assertNotIn("<!-- Chart.js is injected by Rust before page load -->", report)


if __name__ == "__main__":
    unittest.main()
