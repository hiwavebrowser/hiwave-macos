#!/usr/bin/env python3
"""test_receipt_no_host_paths.py — receipt.py output goes into public PR bodies and
comments, so it must not carry the paths of the machine that ran it.

A file inside the repository prints repo-relative; a file outside it prints as its
basename. --host-paths keeps the old absolute form for local use.

Pure Python standard library only (compatible with script-guards CI lane).
Run: python3 scripts/tests/test_receipt_no_host_paths.py
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parent.parent
REPO = SCRIPTS.parent
RECEIPT = SCRIPTS / "receipt.py"


def run_receipt(*args, cwd=REPO):
    out = subprocess.run([sys.executable, str(RECEIPT), "--package", "T", *args],
                         capture_output=True, text=True, cwd=cwd, timeout=120)
    assert out.returncode == 0, out.stderr
    return out.stdout


class TestReceiptNoHostPaths(unittest.TestCase):

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        # resolve(): macOS hands out /var/... which is a link to /private/var/...
        self.outside = Path(self.tmp.name).resolve()
        (self.outside / "parity").mkdir()
        self.binary = self.outside / "parity" / "parity-capture"
        self.binary.write_bytes(b"not a real binary")
        self.campaign = self.outside / "parity_test_results.json"
        self.campaign.write_text(json.dumps({"results": [{"pixel": {"diffPercent": 1.5}}], "passed": 1, "failed": 0}))
        self.run_file = self.outside / "ab_table.txt"
        self.run_file.write_text("row\n")

    def tearDown(self):
        self.tmp.cleanup()

    def host_strings(self):
        return [str(self.outside), self.tmp.name, str(REPO), str(Path.home())]

    def assert_clean(self, text):
        for s in self.host_strings():
            self.assertNotIn(s, text, "host path in receipt output")

    def test_files_outside_the_repo_print_as_basenames(self):
        out = run_receipt("--binary", str(self.binary), "--campaign", str(self.campaign),
                          "--runs", str(self.run_file))
        self.assert_clean(out)
        rec = json.loads(out)
        self.assertEqual(rec["binary"], "parity-capture")
        self.assertEqual(rec["campaign"]["file"], "parity_test_results.json")
        self.assertEqual(rec["raw_runs"][0]["file"], "ab_table.txt")
        # the facts about the files are unchanged
        self.assertEqual(rec["profile"], "parity")
        self.assertEqual(rec["campaign"]["cases"], 1)
        self.assertEqual(rec["raw_runs"][0]["bytes"], 4)
        self.assertIsNotNone(rec["binary_sha256"])

    def test_files_inside_the_repo_print_repo_relative(self):
        out = run_receipt("--binary", str(RECEIPT), "--runs", "scripts/receipt.py", str(REPO / "Cargo.lock"))
        self.assert_clean(out)
        rec = json.loads(out)
        self.assertEqual(rec["binary"], "scripts/receipt.py")
        self.assertEqual([r["file"] for r in rec["raw_runs"]], ["scripts/receipt.py", "Cargo.lock"])

    def test_relative_arguments_are_resolved_from_the_working_directory(self):
        out = run_receipt("--runs", "receipt.py", cwd=SCRIPTS)
        self.assertEqual(json.loads(out)["raw_runs"][0]["file"], "scripts/receipt.py")

    def test_a_missing_file_still_prints_no_host_path(self):
        out = run_receipt("--runs", str(self.outside / "gone.txt"))
        self.assert_clean(out)
        rec = json.loads(out)
        self.assertEqual(rec["raw_runs"][0], {"file": "gone.txt", "sha256": None, "bytes": None})

    def test_a_campaign_that_does_not_parse_prints_no_host_path(self):
        self.campaign.write_text("{")
        out = run_receipt("--campaign", str(self.campaign))
        self.assert_clean(out)
        self.assertEqual(json.loads(out)["campaign"]["file"], "parity_test_results.json")

    def test_paths_typed_into_the_note_are_stripped(self):
        note = f"bins at {self.binary} and {REPO / 'scripts' / 'receipt.py'}; see {self.run_file}."
        out = run_receipt("--note", note)
        self.assert_clean(out)
        self.assertEqual(json.loads(out)["note"], "bins at parity-capture and scripts/receipt.py; see ab_table.txt.")

    def test_note_text_that_is_not_a_host_path_is_left_alone(self):
        sys.path.insert(0, str(SCRIPTS))
        import receipt
        for t in ["quiet board 27/60 and/or 32/78, ratio 1.5/2.5",
                  "see https://github.com/hiwavebrowser/hiwave-macos/pull/660 and /search?q=x",
                  "git@github.com:hiwavebrowser/hiwave-macos.git",
                  "./rel/x.txt ../up/y.txt a/b/c"]:
            self.assertEqual(receipt.public_text(t, REPO), t)

    def test_other_spellings_of_a_host_path_in_the_note(self):
        sys.path.insert(0, str(SCRIPTS))
        import receipt
        home = Path.home()
        for t, want in [(r"built at C:\Users\pete\hiwave\target\parity\parity-capture.exe.", "built at parity-capture.exe."),
                        ("D:/a/b.txt, then", "b.txt, then"),
                        (f"file://{home}/frames/y.html", "y.html"),
                        ("~/zz-not-a-repo/a/b.txt", "b.txt"),
                        (f"dir {home}/bins/ (and {home}/x/c.txt)", "dir bins (and c.txt)")]:
            self.assertEqual(receipt.public_text(t, REPO), want)

    def test_markdown_output_is_clean_too(self):
        out = run_receipt("--markdown", "--binary", str(self.binary), "--runs", str(self.run_file))
        self.assert_clean(out)

    def test_host_paths_flag_keeps_the_absolute_form(self):
        out = run_receipt("--host-paths", "--binary", str(self.binary), "--runs", str(self.run_file))
        rec = json.loads(out)
        self.assertEqual(rec["binary"], str(self.binary))
        self.assertEqual(rec["raw_runs"][0]["file"], str(self.run_file))


if __name__ == "__main__":
    unittest.main()
