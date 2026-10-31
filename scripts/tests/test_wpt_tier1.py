"""T14 (W0b): the WPT reftest runner may not score a frame it cannot vouch for.

Pins the holes the first network sync found:
- `css-flexbox/align-self-015` links `support/flexbox.css`, which the manifest
  never listed. `--check` passed and both sides rendered without the sheet.
- The "no script host" skip outlived the script host (#799 --run-inline-scripts).
- last-run.json named no develop SHA and no oracle.

Run: python3 scripts/tests/test_wpt_tier1.py
"""
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

REPO = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts"))
import wpt_tier1  # noqa: E402


class MissingResources(unittest.TestCase):
    def page(self, html, files=()):
        root = pathlib.Path(self.td.name)
        for rel in files:
            target = root / rel
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text("")
        page = root / "css" / "t.html"
        page.parent.mkdir(parents=True, exist_ok=True)
        page.write_text(html)
        return page, root

    def setUp(self):
        self.td = tempfile.TemporaryDirectory()
        self.addCleanup(self.td.cleanup)

    def test_absent_relative_stylesheet_is_named(self):
        page, root = self.page('<link href="support/x.css" rel="stylesheet">')
        self.assertEqual(wpt_tier1.missing_resources(page, root), ["support/x.css"])

    def test_present_stylesheet_is_not_named(self):
        page, root = self.page(
            '<link rel=stylesheet href="support/x.css">', files=["css/support/x.css"]
        )
        self.assertEqual(wpt_tier1.missing_resources(page, root), [])

    def test_root_absolute_resolves_against_the_wpt_root(self):
        page, root = self.page(
            '<link rel="stylesheet" href="/fonts/a.css"><img src="/fonts/b.png">',
            files=["fonts/a.css"],
        )
        self.assertEqual(wpt_tier1.missing_resources(page, root), ["/fonts/b.png"])

    def test_metadata_links_and_remote_urls_are_not_resources(self):
        page, root = self.page(
            '<link rel="author" href="mailto:a@b.c">'
            '<link rel="help" href="https://drafts.csswg.org/x">'
            '<link rel="match" href="absent-ref.html">'
            '<img src="data:image/png;base64,AAAA">'
            '<style>a { background: url(https://x.test/a.png) }</style>'
        )
        self.assertEqual(wpt_tier1.missing_resources(page, root), [])

    def test_css_url_and_import_are_resources(self):
        page, root = self.page(
            '<style>@import "a.css"; b { background: url(c.png) }</style>'
        )
        self.assertEqual(wpt_tier1.missing_resources(page, root), ["a.css", "c.png"])

    def test_a_linked_sheet_is_followed_relative_to_itself(self):
        page, root = self.page(
            '<link rel="stylesheet" href="support/x.css">', files=["css/support/x.css"]
        )
        (root / "css/support/x.css").write_text('a { background: url("y.png") }')
        self.assertEqual(wpt_tier1.missing_resources(page, root), ["y.png"])

    def test_unquoted_missing_sheet_is_named(self):
        page, root = self.page("<link rel=stylesheet href=support/x.css>")
        self.assertEqual(wpt_tier1.missing_resources(page, root), ["support/x.css"])

    def test_unquoted_missing_image_is_named(self):
        page, root = self.page("<img src=missing.png>")
        self.assertEqual(wpt_tier1.missing_resources(page, root), ["missing.png"])

    def test_unquoted_value_ends_at_the_tag(self):
        page, root = self.page("<img src=a.png><img alt=x src=b.png width=1>", files=["css/a.png"])
        self.assertEqual(wpt_tier1.missing_resources(page, root), ["b.png"])

    def test_url_in_a_comment_is_not_a_resource(self):
        page, root = self.page(
            "<!-- <img src=gone.png> url(gone2.png) -->"
            "<style>/* url(gone3.png) */ a { color: red }</style>"
        )
        self.assertEqual(wpt_tier1.missing_resources(page, root), [])

    def test_url_in_a_linked_sheets_comment_is_not_a_resource(self):
        page, root = self.page('<link rel="stylesheet" href="x.css">', files=["css/x.css"])
        (root / "css/x.css").write_text("/* url(gone.png) */ a { color: red }")
        self.assertEqual(wpt_tier1.missing_resources(page, root), [])

    def test_meta_content_is_not_a_resource(self):
        page, root = self.page('<meta name="assert" content="uses url(gone.png) here">')
        self.assertEqual(wpt_tier1.missing_resources(page, root), [])

    def test_data_src_is_not_a_resource(self):
        page, root = self.page('<img data-src="gone.png" src="here.png">', files=["css/here.png"])
        self.assertEqual(wpt_tier1.missing_resources(page, root), [])

    def test_a_script_assignment_is_not_a_resource(self):
        page, root = self.page('<script>img.src = next; a.src="gone.png";</script>')
        self.assertEqual(wpt_tier1.missing_resources(page, root), [])

    def test_a_script_element_src_is_a_resource(self):
        page, root = self.page("<script src=gone.js></script>")
        self.assertEqual(wpt_tier1.missing_resources(page, root), ["gone.js"])


class Unquoted(unittest.TestCase):
    """The rest of the runner reads the same attributes the gate does."""

    def test_rel_match_href_may_be_unquoted(self):
        with tempfile.TemporaryDirectory() as td:
            page = pathlib.Path(td) / "t.html"
            page.write_text("<link rel=match href=t-ref.html>")
            self.assertEqual(wpt_tier1.rel_match_hrefs(page), ["t-ref.html"])

    def test_root_absolute_unquoted_href_is_staged(self):
        with tempfile.TemporaryDirectory() as td:
            page = pathlib.Path(td) / "t.html"
            page.write_text("<link rel=stylesheet href=/fonts/absent.css><img src=/x.png>")
            staged = wpt_tier1.stage_html(page).read_text()
            self.assertNotIn("href=/fonts", staged)
            self.assertIn(str(wpt_tier1.WPT_ROOT / "fonts/absent.css"), staged)
            self.assertIn(str(wpt_tier1.WPT_ROOT / "x.png"), staged)

    def test_unquoted_fonts_href_declares_a_web_font(self):
        self.assertTrue(wpt_tier1.webfont_dependent("<link rel=stylesheet href=/fonts/ahem.css>", ""))
        self.assertTrue(wpt_tier1.webfont_dependent("<meta name=flags content=ahem>", ""))

    def test_unquoted_fuzzy_is_unrunnable(self):
        self.assertIn("fuzzy", wpt_tier1.unrunnable_reason("<meta name=fuzzy content=1>", ""))


class CampaignDiffReasons(unittest.TestCase):
    """Every way the reported diff can be missing says which way it was."""

    def setUp(self):
        self.td = tempfile.TemporaryDirectory()
        self.addCleanup(self.td.cleanup)
        oracle = pathlib.Path(self.td.name)
        (oracle / "node_modules").mkdir()
        patcher = mock.patch.object(wpt_tier1, "ORACLE_DIR", oracle)
        patcher.start()
        self.addCleanup(patcher.stop)
        self.oracle = oracle

    def diff(self, returncode=0, stdout="", stderr="", raises=None):
        done = subprocess.CompletedProcess([], returncode, stdout, stderr)
        with mock.patch.object(wpt_tier1.subprocess, "run", return_value=done, side_effect=raises):
            return wpt_tier1.campaign_diff(pathlib.Path("t"), pathlib.Path("r"))

    def test_a_measurement_has_no_reason(self):
        self.assertEqual(self.diff(stdout='{"diff_pct": 1.23456}'), (1.2346, None))

    def test_no_node(self):
        pct, why = self.diff(raises=FileNotFoundError("node"))
        self.assertIsNone(pct)
        self.assertIn("node", why)

    def test_no_node_modules(self):
        (self.oracle / "node_modules").rmdir()
        pct, why = self.diff(stdout='{"diff_pct": 1}')
        self.assertIsNone(pct)
        self.assertIn("node_modules", why)

    def test_a_crash_says_its_exit_and_last_line(self):
        pct, why = self.diff(returncode=3, stderr="at x\nTypeError: boom\n")
        self.assertIsNone(pct)
        self.assertIn("exit 3", why)
        self.assertIn("boom", why)

    def test_a_timeout(self):
        pct, why = self.diff(raises=subprocess.TimeoutExpired("node", 60))
        self.assertIsNone(pct)
        self.assertIn("timeout", why)

    def test_bad_json(self):
        pct, why = self.diff(stdout="not json")
        self.assertIsNone(pct)
        self.assertIn("JSON", why)

    def test_json_that_is_not_an_object_does_not_raise(self):
        pct, why = self.diff(stdout="[1, 2]")
        self.assertIsNone(pct)
        self.assertIn("diff_pct", why)

    def test_a_non_number(self):
        pct, why = self.diff(stdout='{"diff_pct": "x"}')
        self.assertIsNone(pct)
        self.assertIn("diff_pct", why)

    def test_a_bool_is_not_a_number(self):
        pct, why = self.diff(stdout='{"diff_pct": true}')
        self.assertIsNone(pct)
        self.assertIn("diff_pct", why)


class GatePrecedence(unittest.TestCase):
    def test_a_missing_sheet_outranks_reftest_wait(self):
        update = wpt_tier1.gate("<html class=reftest-wait>", "", ["support/x.css"])
        self.assertEqual(update["status"], "ERROR")
        self.assertIn("support/x.css", update["reason"])

    def test_reftest_wait_with_its_resources_is_skip(self):
        self.assertEqual(wpt_tier1.gate("<html class=reftest-wait>", "", [])["status"], "SKIP")

    def test_a_page_with_nothing_wrong_goes_on_to_pixels(self):
        self.assertIsNone(wpt_tier1.gate("<p>x</p>", "<p>x</p>", []))


class Unproven(unittest.TestCase):
    MANIFEST = json.loads((REPO / "trench/wpt/MANIFEST.json").read_text())

    def test_the_eight_t14_probe_survivors_are_marked(self):
        marked = {e["id"] for e in self.MANIFEST["entries"] if e.get("unproven")}
        self.assertEqual(
            marked,
            {
                "css-text/overflow-wrap/overflow-wrap-004",
                "css-text/word-break/break-word-overflow-wrap-interactions",
                "css-inline/br-font-size",
                "css-inline/br-line-height",
                "css-inline/empty-text-node-001",
                "css-flexbox/align-self-015",
                "css-flexbox/align-items-baseline-overflow-non-visible",
                "css-text/word-break/break-boundary-2-chars-002",
            },
        )

    def test_only_a_pass_is_unproven(self):
        cases = [
            {"id": "a", "status": "PASS"},
            {"id": "b", "status": "FAIL"},
            {"id": "c", "status": "PASS"},
        ]
        entries = {"a": {"unproven": "why"}, "b": {"unproven": "why"}, "c": {}}
        self.assertEqual(wpt_tier1.unproven_passes(cases, entries), {"a": "why"})


class Runnability(unittest.TestCase):
    def test_inline_script_without_a_wait_is_scored(self):
        self.assertIsNone(wpt_tier1.unrunnable_reason("<script>a()</script>", ""))

    def test_reftest_wait_stays_unscored(self):
        reason = wpt_tier1.unrunnable_reason("<html class=reftest-wait>", "")
        self.assertIn("reftest-wait", reason)
        self.assertNotIn("no script host", reason)

    def test_capture_turns_script_on_only_for_a_page_with_a_script(self):
        with tempfile.TemporaryDirectory() as td:
            page = pathlib.Path(td) / "p.html"
            page.write_text("<SCRIPT>a()</SCRIPT>")
            self.assertIn("--run-inline-scripts", wpt_tier1.capture_args(page, page, {"width": 1, "height": 1}))
            page.write_text("<p>no script</p>")
            self.assertNotIn("--run-inline-scripts", wpt_tier1.capture_args(page, page, {"width": 1, "height": 1}))


class Provenance(unittest.TestCase):
    def test_capture_binary_is_the_parity_profile(self):
        self.assertEqual(wpt_tier1.capture_bin({}), REPO / "target/parity/parity-capture")

    def test_develop_sha_is_never_head_by_default(self):
        # Its own repository: with TMPDIR inside a checkout, a bare temp dir
        # finds that checkout's origin/develop.
        with tempfile.TemporaryDirectory() as td:
            subprocess.run(["git", "init", "-q", td], check=True)
            self.assertIsNone(wpt_tier1.develop_sha_at(pathlib.Path(td)))

    def test_capture_binary_honours_cargo_target_dir(self):
        self.assertEqual(
            wpt_tier1.capture_bin({"CARGO_TARGET_DIR": "/elsewhere/t"}),
            pathlib.Path("/elsewhere/t/parity/parity-capture"),
        )

    def test_a_relative_cargo_target_dir_is_from_the_repo(self):
        self.assertEqual(
            wpt_tier1.capture_bin({"CARGO_TARGET_DIR": "t2"}), REPO / "t2/parity/parity-capture"
        )

    def test_capture_binary_label_survives_a_target_outside_the_repo(self):
        self.assertEqual(
            wpt_tier1.capture_label(pathlib.Path("/elsewhere/t/parity/parity-capture")),
            "/elsewhere/t/parity/parity-capture",
        )
        self.assertEqual(
            wpt_tier1.capture_label(REPO / "target/parity/parity-capture"),
            "target/parity/parity-capture",
        )

    def test_oracle_names_both_rules(self):
        oracle = wpt_tier1.oracle_record()
        self.assertEqual(oracle["status_rule"]["max_diff_pct"], 0.0)
        self.assertEqual(oracle["campaign_rule"]["threshold_pct"], 15)
        self.assertEqual(oracle["campaign_rule"]["pixelmatch_threshold"], 0.1)
        json.dumps(oracle)

    def test_campaign_counts_never_grade_a_missing_measurement(self):
        cases = [
            {"status": "FAIL", "campaign_diff_pct": 3.0},
            {"status": "PASS", "campaign_diff_pct": 0.0},
            {"status": "FAIL", "campaign_diff_pct": 40.0},
            {"status": "FAIL", "campaign_diff_pct": None},
            {"status": "SKIP", "campaign_diff_pct": None},
        ]
        self.assertEqual(
            wpt_tier1.campaign_counts(cases), {"within": 2, "over": 1, "not_measured": 1}
        )


class Manifest(unittest.TestCase):
    def test_flexbox_support_sheet_is_listed(self):
        manifest = json.loads((REPO / "trench/wpt/MANIFEST.json").read_text())
        self.assertIn("css/css-flexbox/support/flexbox.css", manifest["support_paths"]["paths"])

    def test_every_manifest_page_has_its_resources_when_synced(self):
        if not wpt_tier1.WPT_ROOT.is_dir():
            self.skipTest("third_party/wpt not synced")
        manifest = json.loads((REPO / "trench/wpt/MANIFEST.json").read_text())
        for entry in manifest["entries"]:
            for rel in (entry["path"], entry["ref"]):
                page = wpt_tier1.WPT_ROOT / rel
                if page.exists():
                    self.assertEqual(wpt_tier1.missing_resources(page, wpt_tier1.WPT_ROOT), [], rel)

    def test_sync_fetches_without_blobs(self):
        text = (REPO / "scripts/wpt_sync.sh").read_text()
        self.assertIn("--filter=blob:none", text)
        self.assertNotIn("NOT YET RUN", text)

    def test_sync_check_passes_or_names_what_is_missing(self):
        proc = subprocess.run(
            ["bash", str(REPO / "scripts/wpt_sync.sh"), "--dry-run"], capture_output=True, text=True
        )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("css/css-flexbox/support/flexbox.css", proc.stdout)


if __name__ == "__main__":
    unittest.main()
