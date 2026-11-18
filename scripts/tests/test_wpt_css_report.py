import sys
import unittest
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from wpt_css_report import summarize


def run(status, *tests):
    return {"status": status, "subtests": [
        {"name": name, "status": outcome, "message": message} for name, outcome, message in tests
    ]}


class CssReportTests(unittest.TestCase):
    def test_counts_crashes_without_linking_paths(self):
        row = {"page": "/css/css-color/private.html", "chrome": run("OK", ("x", "PASS", None)),
               "hiwave": run("CRASH")}
        counts, clusters = summarize([row])
        self.assertEqual(counts["css/css-color"]["hiwave_crashes"], 1)
        self.assertEqual(counts["css/css-color"]["total"], 1)
        self.assertFalse(clusters)

    def test_only_reference_passing_subtests_enter_clusters(self):
        fail = ("paint", "FAIL", "CSSStyleRule is not defined")
        row = {"page": "/css/cssom/rules.html", "chrome": run("OK", fail), "hiwave": run("OK", fail)}
        self.assertFalse(summarize([row])[1])

    def test_first_membership_assertion_precedes_value_serialization(self):
        check = "Property color value 'red'"
        row = {"page": "/css/css-color/example.html",
               "chrome": run("OK", (check, "PASS", None)),
               "hiwave": run("OK", (check, "FAIL", "assert_true: color doesn't seem to be supported in the computed style"))}
        counts, clusters = summarize([row])
        self.assertEqual(counts["css/css-color"]["classified_fail"], 1)
        self.assertEqual(next(iter(clusters))[0], "Property membership on getComputedStyle fails")

    def test_duplicate_names_are_matched_by_occurrence(self):
        row = {"page": "/css/cssom/example.html",
               "chrome": run("OK", ("rule", "FAIL", None), ("rule", "PASS", None)),
               "hiwave": run("OK", ("rule", "FAIL", "CSSStyleRule is not defined"),
                             ("rule", "FAIL", "CSSStyleRule is not defined"))}
        counts, clusters = summarize([row])
        self.assertEqual(counts["css/cssom"]["matched_fail"], 1)
        self.assertEqual(next(iter(clusters.values()))["count"], 1)

    def test_origin_port_does_not_prevent_matching(self):
        row = {"page": "/css/cssom/example.html",
               "chrome": run("OK", ("expected http://127.0.0.1:51000/style.css", "PASS", None)),
               "hiwave": run("OK", ("expected http://127.0.0.1:52000/style.css", "FAIL", "CSSStyleRule is not defined"))}
        counts, _ = summarize([row])
        self.assertEqual(counts["css/cssom"]["matched_fail"], 1)
