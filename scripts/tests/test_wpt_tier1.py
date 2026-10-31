"""T14 (W0b): the WPT reftest runner may not score a frame it cannot vouch for.

Pins the holes the first network sync found:
- `css-flexbox/align-self-015` links `support/flexbox.css`, which the manifest
  never listed. `--check` passed and both sides rendered without the sheet.
- The "no script host" skip outlived the script host (#799 --run-inline-scripts).
- last-run.json named no develop SHA and no oracle.

Run: python3 scripts/tests/test_wpt_tier1.py
"""
import json
import pathlib
import subprocess
import sys
import tempfile
import unittest

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
        self.assertEqual(
            wpt_tier1.CAPTURE_BIN.relative_to(REPO).as_posix(), "target/parity/parity-capture"
        )

    def test_develop_sha_is_never_head_by_default(self):
        self.assertIsNone(wpt_tier1.develop_sha_at(pathlib.Path(tempfile.gettempdir())))

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
