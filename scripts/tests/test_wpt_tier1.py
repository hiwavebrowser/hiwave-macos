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
import wpt_import  # noqa: E402
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

    def test_src_inside_another_attributes_quotes_is_not_a_resource(self):
        page, root = self.page("""<div title="src=gone.png"></div><p data-x='src="gone2.png"'></p>""")
        self.assertEqual(wpt_tier1.missing_resources(page, root), [])

    def test_a_close_angle_inside_quotes_does_not_end_the_tag(self):
        page, root = self.page('<img alt="a>b" src=gone.png>')
        self.assertEqual(wpt_tier1.missing_resources(page, root), ["gone.png"])

    def test_a_close_angle_inside_quotes_does_not_end_a_link(self):
        page, root = self.page('<link title="a>b" rel=stylesheet href=gone.css>')
        self.assertEqual(wpt_tier1.missing_resources(page, root), ["gone.css"])

    def test_page_text_is_not_a_resource(self):
        page, root = self.page("<p>write url(gone.png) in your sheet</p>")
        self.assertEqual(wpt_tier1.missing_resources(page, root), [])

    def test_a_style_attribute_is_scanned(self):
        page, root = self.page('<div style="background: url(gone.png)"></div>')
        self.assertEqual(wpt_tier1.missing_resources(page, root), ["gone.png"])

    def test_local_resources_lists_present_files_for_the_importer(self):
        page, root = self.page(
            '<link rel="stylesheet" href="support/x.css"><img src="here.png">',
            files=["css/support/x.css", "css/here.png"],
        )
        (root / "css/support/x.css").write_text("a { background: url(y.png) }")
        (root / "css/support/y.png").write_text("")
        found = [t.relative_to(root).as_posix() for _, t in wpt_tier1.local_resources(page, root)]
        self.assertEqual(sorted(found), ["css/here.png", "css/support/x.css", "css/support/y.png"])


class Unquoted(unittest.TestCase):
    """The rest of the runner reads the same attributes the gate does."""

    def test_rel_match_href_may_be_unquoted(self):
        with tempfile.TemporaryDirectory() as td:
            page = pathlib.Path(td) / "t.html"
            page.write_text("<link rel=match href=t-ref.html>")
            self.assertEqual(wpt_tier1.rel_match_hrefs(page), ["t-ref.html"])

    def test_rel_match_with_a_close_angle_in_quotes(self):
        with tempfile.TemporaryDirectory() as td:
            page = pathlib.Path(td) / "t.html"
            page.write_text('<link title="a>b" rel="match" href="t-ref.html"><link rel=mismatch href=no.html>')
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


class Proof(unittest.TestCase):
    """A PASS has to go red under a mutation, or it leaves the pass count."""

    def setUp(self):
        self.td = tempfile.TemporaryDirectory()
        self.addCleanup(self.td.cleanup)
        self.dir = pathlib.Path(self.td.name)
        self.page = self.dir / "t.html"
        self.page.write_text('<style>a { color: red }</style><p style="margin:0">a b</p>')
        self.ref = self.dir / "ref.ppm"

    def prove(self, entry, diffs, capture_says=None):
        """`diffs` is what each probe's frame differs from the reference by."""
        frames = iter(diffs)
        with mock.patch.object(wpt_tier1, "capture", return_value=capture_says), mock.patch.object(
            wpt_tier1, "compare_frames", side_effect=lambda *_: (next(frames), 0.0)
        ):
            return wpt_tier1.prove_pass(entry, self.page, self.ref, {"width": 1, "height": 1}, self.dir)

    def test_a_page_that_goes_red_without_style_is_proven(self):
        self.assertIsNone(self.prove({}, [100]))

    def test_a_page_that_matches_without_style_is_unproven(self):
        self.assertIn("style removed", self.prove({}, [0]))

    def test_a_mutation_that_changes_nothing_is_unproven(self):
        entry = {"mutations": [{"find": "a b", "replace": "a c"}]}
        self.assertIn("a b", self.prove(entry, [100, 0]))

    def test_every_mutation_must_go_red(self):
        entry = {"mutations": [{"find": "a b", "replace": "a c"}, {"find": "color", "replace": "margin"}]}
        self.assertIsNone(self.prove(entry, [100, 5, 5]))
        self.assertIn("color", self.prove(entry, [100, 5, 0]))

    def test_a_mutation_that_no_longer_applies_is_unproven_and_says_so(self):
        entry = {"mutations": [{"find": "not on the page", "replace": "x"}]}
        self.assertIn("does not apply", self.prove(entry, [100]))

    def test_a_probe_that_cannot_render_is_unproven(self):
        self.assertIn("no frame", self.prove({}, [], capture_says="capture exit 1"))

    def test_style_removed_takes_sheets_elements_and_attributes(self):
        out = wpt_tier1.style_removed(
            '<link rel="stylesheet" href="a.css"><link rel="match" href="r.html">'
            "<STYLE>a { }</STYLE><p style='x'>t</p><p title=\"style=y\">u</p>"
        )
        self.assertNotIn("a.css", out)
        self.assertNotIn("<STYLE", out)
        self.assertNotIn("'x'", out)
        self.assertIn("r.html", out)
        self.assertIn('title="style=y"', out)

    def test_apply_mutation_replaces_every_occurrence(self):
        self.assertEqual(wpt_tier1.apply_mutation("aXbX", {"find": "X", "replace": "y"}), "ayby")
        self.assertIsNone(wpt_tier1.apply_mutation("ab", {"find": "X", "replace": "y"}))


class Counts(unittest.TestCase):
    CASES = [
        {"id": "a", "status": "PASS", "area": "x"},
        {"id": "b", "status": "UNPROVEN", "area": "x", "reason": "why"},
        {"id": "c", "status": "FAIL", "area": "y"},
        {"id": "d", "status": "SKIP", "area": "y"},
        {"id": "e", "status": "ERROR", "area": "y"},
    ]

    def test_an_unproven_case_leaves_the_pass_count(self):
        counts = wpt_tier1.count_statuses(self.CASES)
        self.assertEqual(counts, {"PASS": 1, "FAIL": 1, "UNPROVEN": 1, "SKIP": 1, "ERROR": 1})

    def test_the_rate_keeps_an_unproven_case_in_the_denominator(self):
        self.assertEqual(wpt_tier1.pass_rate(wpt_tier1.count_statuses(self.CASES)), round(1 / 3, 4))

    def test_per_area_table(self):
        areas = wpt_tier1.per_area(self.CASES)
        self.assertEqual(areas["x"], {"PASS": 1, "FAIL": 0, "UNPROVEN": 1, "SKIP": 0, "ERROR": 0})
        self.assertEqual(areas["y"], {"PASS": 0, "FAIL": 1, "UNPROVEN": 0, "SKIP": 1, "ERROR": 1})

    def test_unproven_passes_carries_the_reason(self):
        self.assertEqual(wpt_tier1.unproven_passes(self.CASES), {"b": "why"})

    def test_campaign_counts_score_an_unproven_case(self):
        cases = [{"status": "UNPROVEN", "campaign_diff_pct": 0.0}]
        self.assertEqual(wpt_tier1.campaign_counts(cases), {"within": 1, "over": 0, "not_measured": 0})


class Staging(unittest.TestCase):
    def test_a_shared_reference_is_staged_once(self):
        with tempfile.TemporaryDirectory() as td:
            page = pathlib.Path(td) / "ref.html"
            page.write_text("<p>one</p>")
            first = wpt_tier1.stage_html(page)
            first_mtime = first.stat().st_mtime_ns
            page.write_text("<p>two</p>")
            self.assertEqual(wpt_tier1.stage_html(page), first)
            self.assertEqual(first.stat().st_mtime_ns, first_mtime)


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

    def test_an_empty_cargo_target_dir_is_the_default(self):
        self.assertEqual(
            wpt_tier1.capture_bin({"CARGO_TARGET_DIR": ""}), REPO / "target/parity/parity-capture"
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


EIGHT = {
    "css-text/overflow-wrap/overflow-wrap-004",
    "css-text/word-break/break-word-overflow-wrap-interactions",
    "css-inline/br-font-size",
    "css-inline/br-line-height",
    "css-inline/empty-text-node-001",
    "css-flexbox/align-self-015",
    "css-flexbox/align-items-baseline-overflow-non-visible",
    "css-text/word-break/break-boundary-2-chars-002",
}


class Coverage(unittest.TestCase):
    MANIFEST = json.loads((REPO / "trench/wpt/MANIFEST.json").read_text())

    def test_each_of_the_eight_has_a_second_mutation(self):
        marked = {e["id"] for e in self.MANIFEST["entries"] if e.get("mutations")}
        self.assertTrue(EIGHT <= marked, sorted(EIGHT - marked))

    def test_no_entry_is_marked_unproven_by_hand(self):
        self.assertEqual([e["id"] for e in self.MANIFEST["entries"] if e.get("unproven")], [])

    def test_every_named_area_has_cases_and_a_row(self):
        areas = {}
        for e in self.MANIFEST["entries"]:
            if e.get("area"):
                areas.setdefault(e["area"], []).append(e)
        self.assertEqual(sorted(areas), sorted(wpt_import.AREAS))
        for name, entries in areas.items():
            self.assertGreaterEqual(len(entries), 10, name)
            self.assertEqual({e["maps_to"] for e in entries}, {wpt_import.AREAS[name]["row"]}, name)

    def test_the_seed_is_kept(self):
        self.assertEqual(sum(1 for e in self.MANIFEST["entries"] if not e.get("area")), 30)

    def test_ids_are_unique(self):
        ids = [e["id"] for e in self.MANIFEST["entries"]]
        self.assertEqual(len(ids), len(set(ids)))

    def test_mutations_apply_to_the_page_when_synced(self):
        if not (wpt_tier1.WPT_ROOT / "css").is_dir():
            self.skipTest("third_party/wpt not synced")
        for e in self.MANIFEST["entries"]:
            text = (wpt_tier1.WPT_ROOT / e["path"]).read_text(errors="replace")
            for m in e.get("mutations", []):
                self.assertIn(m["find"], text, e["id"])


class Importer(unittest.TestCase):
    def setUp(self):
        self.td = tempfile.TemporaryDirectory()
        self.addCleanup(self.td.cleanup)
        self.root = pathlib.Path(self.td.name)

    def write(self, rel, text=""):
        target = self.root / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)

    def reftest(self, rel, ref="t-ref.html", extra=""):
        self.write(rel, f'<link rel="match" href="{ref}">{extra}<p>x</p>')
        self.write(str(pathlib.Path(rel).parent / ref), "<p>x</p>")

    def test_a_plain_reftest_is_usable_with_its_reference(self):
        self.reftest("css/a/t.html")
        self.assertEqual(wpt_import.usable_ref(self.root / "css/a/t.html", self.root), "css/a/t-ref.html")

    def test_what_the_runner_cannot_score_is_not_imported(self):
        self.reftest("css/a/w.html", extra='<html class="reftest-wait">')
        self.reftest("css/a/f.html", extra='<meta name="fuzzy" content="1">')
        self.reftest("css/a/m.html", extra='<link rel="mismatch" href="t-ref.html">')
        self.reftest("css/a/two.html", extra='<link rel="match" href="t-ref.html">')
        self.write("css/a/noref.html", "<p>x</p>")
        self.reftest("css/a/gone.html", extra='<img src="absent.png">')
        self.write("css/a/noreftarget.html", '<link rel="match" href="absent-ref.html">')
        for name in ("w", "f", "m", "two", "noref", "gone", "noreftarget"):
            self.assertIsNone(wpt_import.usable_ref(self.root / f"css/a/{name}.html", self.root), name)

    def test_candidates_leave_out_references_support_and_staged_copies(self):
        for rel in (
            "css/a/t.html",
            "css/a/t-ref.html",
            "css/a/reference/r.html",
            "css/a/support/s.html",
            "css/a/t.__staged.html",
            "css/a/t.tentative.html",
            "css/a/t.xht",
        ):
            self.write(rel)
        found = wpt_import.candidates(self.root, {"dir": "css/a", "include": "."})
        self.assertEqual(found, ["css/a/t.html"])

    def test_include_filters_on_the_path_inside_the_area(self):
        self.write("css/a/sticky/one.html")
        self.write("css/a/two.html")
        found = wpt_import.candidates(self.root, {"dir": "css/a", "include": "^sticky/"})
        self.assertEqual(found, ["css/a/sticky/one.html"])

    def test_the_spread_is_even_and_stable(self):
        names = [f"n{i:02}" for i in range(10)]
        self.assertEqual(wpt_import.spread(names, 3), ["n00", "n03", "n06"])
        self.assertEqual(wpt_import.spread(names, 20), names)

    def test_support_closure_excludes_the_test_and_ref(self):
        self.reftest("css/a/t.html", extra='<link rel="stylesheet" href="s.css">')
        self.write("css/a/s.css", "a { background: url(i.png) }")
        self.write("css/a/i.png")
        self.assertEqual(
            wpt_import.support_of(["css/a/t.html", "css/a/t-ref.html"], self.root),
            ["css/a/i.png", "css/a/s.css"],
        )


if __name__ == "__main__":
    unittest.main()
