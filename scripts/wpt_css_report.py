"""Aggregate a complete paired run. Cluster only matched, named assertions."""
import argparse

from wpt_subtest_diff import normalize_subtest_name
import collections
import json
import re
import subprocess
from pathlib import Path

CSS = "crates/rustkit-css/src/lib.rs"
CSSOM = "crates/rustkit-bindings/src/web_cssom.js"


def diagnosis(test, page):
    name, message = test["name"], test.get("message") or ""
    prop = re.search(r"property <([\w-]+)>|^Property ([\w-]+) value|^e\.style\['([\w-]+)'\]", name)
    prop = next((p for p in prop.groups() if p), None) if prop else None
    if "doesn't seem to be supported in the computed style" in message:
        return ("Property membership on getComputedStyle fails",
                "crates/rustkit-bindings/src/dom.rs makeComputed() Proxy has no has trap")
    if prop:
        if "value should be supported" in message or "is a supported value for" in message:
            return f"CSS.supports rejects {prop} values", CSSOM + " supports() / " + CSS
        if name.startswith("e.style") and "should not set" in name and message.startswith("assert_equals:"):
            return f"Invalid {prop} value retained in declaration", CSS + " property parser / CSSOM declaration setter"
        if name.startswith("e.style") and "property should be set" in message:
            return f"Valid {prop} value missing from declaration", CSS + " property parser / CSSOM declaration setter"
        if name.startswith("e.style") and message.startswith("Colors do not match"):
            if "color-mix(" in name:
                return "Specified color-mix() serialization differs", "rustkit-css color_mix.rs and CSSOM declaration readback"
            if re.search(r"\(from\s", name):
                return "Specified relative-color serialization differs", "rustkit-css color_fn.rs and CSSOM declaration readback"
            return "Specified color serialization differs", "rustkit-css color_fn.rs and CSSOM declaration readback"
        if name.startswith("e.style") and "serialization should" in message:
            return f"Specified {prop} serialization differs", CSSOM + " declaration readback / " + CSS
        if name.startswith("Property ") and message.startswith(("assert_equals:", "assert_in_array:")):
            return f"Computed {prop} value differs", "computed-value resolution and CSSOM serialization"
        if "property <" in name and message.startswith(("assert_equals:", "assert_approx_equals:")):
            return f"Animation or transition samples differ for {prop}", "animation sampling / property interpolation"
    if page.endswith("/CSSStyleRule-set-selectorText.html") and "Invalid CSS selector:" in name and message.startswith("assert_equals:"):
        return "selectorText setter retains invalid selector", CSSOM + " CSSStyleRule.selectorText setter"
    if page.endswith("/serialize-values.html") and "raw inline style declaration" in message:
        return "Inline declaration value readback differs from canonical serialization", "CSSOM declaration readback and CSS value serializers"
    if page.endswith("/serialize-namespaced-type-selectors.html") and message.startswith("assert_equals:"):
        return "Namespace-qualified type selector serializes differently", CSSOM + " selectorText serialization"
    if page.endswith("/cssstyledeclaration-all-shorthand.html") and "getPropertyValue('all')" in name and 'expected ""' in message:
        return "all shorthand retained after longhand overrides", CSSOM + " shorthand reconstruction"
    if "/css-values/" in page:
        math = re.search(r"\b(calc|min|max|clamp|round|mod|rem|sign|abs|sin|cos|tan|acos|asin|atan2?|exp|log|pow|sqrt|hypot)\(", name)
        if math:
            fn = math[1] + "()"
            if "specified value" in message and "serialize" in message:
                return f"Specified {fn} expression does not simplify as expected", CSS + " CSS math simplification / specified serialization"
            if "got the default value instead" in message:
                return f"Valid {fn} expression falls back to the default value", CSS + " CSS math parsing and value resolution"
            if "serialize to the same thing in used values" in message:
                return f"Used {fn} value differs from equivalent expression", CSS + " CSS math evaluation / resolved style"
        if page.endswith("/if-conditionals.html") and "should be substituted" in message:
            return "CSS if() substitution differs", CSS + " conditional substitution during cascade"
        if "non-zero px-based value" in message and "getComputedStyle" in name:
            return "Viewport unit retained instead of resolved CSS pixels", "computed length resolution and CSSOM readback"
        if page.endswith("/attr-all-types.html") and "attr(" in message:
            return "Typed attr() substitution differs", CSS + " attribute substitution and type conversion"
    if page.endswith("/scrollable-overflow-transform-unreachable-region.html") and re.match(r"assert_equals: scroll(Width|Height) expected", message):
        return "Transformed unreachable overflow changes the reported scroll extent", "rustkit-layout overflow geometry / scroll metrics"
    if "cssText serialization should have no value defintions" in message and page.endswith("/test_font_feature_values_parsing.html"):
        return "Empty font feature value definitions survive serialization", CSSOM + " @font-feature-values cssText"
    if "Selection.toString() for math-auto" in name and message.startswith("assert_equals:"):
        return "Selection string misses math-auto text transformation", "selection text extraction and CSS text-transform"
    if page.endswith("/grid-layout-properties.html") and message.startswith("assert_true: expected true got false"):
        return "Property membership on element.style fails for grid properties", "rustkit-bindings CSSStyleDeclaration property exposure"
    if re.search(r"assert_in_array: gridTemplate(?:Columns|Rows) value", message):
        return "Resolved grid tracks differ in computed style", "rustkit-layout grid track sizing / computed style readback"
    if "grid serialization should be canonical" in message:
        return "Grid shorthand readback differs", CSSOM + " grid shorthand serialization"
    if page.endswith("/at-scope-parsing.html") and 'expected "@scope ' in message:
        return "@scope rule serialization differs", CSSOM + " rule cssText"
    if page.endswith("/scope-invalidation.html") and message.startswith("assert_equals:"):
        return "@scope style does not update after DOM change", "CSS scoped matching and style invalidation"
    if page.endswith("/test_font_family_parsing.html"):
        if "shouldn't parse" in message:
            return "Invalid font-family tokens retained in stylesheet", CSS + " font-family parser / CSSOM rule readback"
        if "should parse" in message or "serialization" in message:
            return "Valid font-family parsed or serialized differently", CSS + " font-family parser / CSSOM rule readback"
    if page.endswith("/test_font_feature_values_parsing.html") and "bad rule type expected 14 but got 0" in message:
        return "Font feature values rule has incorrect CSSRule type", CSSOM + " rule type dispatch"
    if page.split("?")[0].endswith("/font-shorthand-serialization-prevention.html") and 'expected "" but got' in message:
        return "Font shorthand serialized despite incompatible longhands", CSSOM + " shorthand readback"
    if message.startswith("assert_equals:") and "data-" in message:
        geometry = re.search(r"\n(offset(?:Top|Left|Width|Height)|client(?:Height|Width)|scroll(?:Height|Width)|width|height) expected", message)
        if geometry:
            parts = page.strip("/").split("/")
            section = parts[1]
            if section == "css-grid" and len(parts) > 3:
                section += "/" + parts[2]
            elif section == "css-flexbox" and "/abspos/" in page:
                section += "/abspos"
            return f"{section} {geometry[1]} differs from check-layout expectation", "rustkit-layout geometry and DOM metric readback"
    api = re.search(r"\b((?:CSS|DOM|WebKit)[a-zA-Z0-9_]+) is not defined", message)
    if api:
        return f"{api[1]} interface unavailable", "rustkit-bindings interface registration"
    if "should throw in querySelector" in message:
        return "Invalid selector accepted by querySelector", "selector parser and DOM query bindings"
    if "should throw in insertRule" in message:
        return "Invalid rule accepted by insertRule", CSSOM + " insertRule()"
    if "CSS.supports() reports the expected value" in message:
        return "CSS.supports(selector()) gives a different result", CSSOM + " supports()"
    return None


def summarize(rows):
    totals = collections.defaultdict(collections.Counter)
    clusters = {}
    for row in rows:
        page = row["page"]
        directory = "/".join(page.strip("/").split("/")[:2])
        a, b = row["hiwave"], row["chrome"]
        counts = totals[directory]
        counts["pages"] += 1
        counts["total"] += max(len(a["subtests"]), len(b["subtests"]))
        for engine, run in (("hiwave", a), ("chrome", b)):
            counts[engine + "_crashes"] += run["status"] == "CRASH"
            counts[engine + "_timeouts"] += run["status"] == "TIMEOUT"
            counts[engine + "_errors"] += run["status"] not in ("OK", "TIMEOUT", "CRASH")
            counts[engine + "_observed"] += len(run["subtests"])
            counts[engine + "_pass"] += sum(t["status"] == "PASS" for t in run["subtests"])
        if a["status"] != "OK" or b["status"] != "OK":
            continue
        reference = collections.defaultdict(collections.deque)
        for test in b["subtests"]:
            reference[normalize_subtest_name(test["name"])].append(test["status"])
        for test in a["subtests"]:
            matches = reference[normalize_subtest_name(test["name"])]
            if not matches or matches.popleft() != "PASS" or test["status"] != "FAIL":
                continue
            counts["matched_fail"] += 1
            key = diagnosis(test, page)
            if key is None:
                continue
            counts["classified_fail"] += 1
            entry = clusters.setdefault(key, {"count": 0, "files": set()})
            entry["count"] += 1
            entry["files"].add(page.split("?")[0].split("#")[0])
    return dict(totals), clusters


def directory_tables(counts):
    lines = ["# Pinned CSS testharness results", "",
             "Windows, Chrome for Testing 148.0.7778.216, WPT a24eb4e7a77917cfa5a6f64e8b02288ca1df1877.",
             "The shared denominator sums the larger reported subtest count per page.", ""]
    for directory, count in sorted(counts.items()):
        lines.extend([f"## `{directory}/`", "",
                      "| Browser | Pages | Reported subtests | Shared denominator | Passes | Pass rate | Crashes |",
                      "|---|---:|---:|---:|---:|---:|---:|"])
        total = count["total"]
        for engine, label in (("hiwave", "HiWave"), ("chrome", "Chrome")):
            passed = count[engine + "_pass"]
            rate = f"{passed / total:.1%}" if total else "n/a"
            lines.append(f'| {label} | {count["pages"]:,} | {count[engine + "_observed"]:,} | '
                         f'{total:,} | {passed:,} | {rate} | {count[engine + "_crashes"]} |')
        lines.append("")
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("inventory", type=Path)
    parser.add_argument("results", type=Path)
    parser.add_argument("expectations", type=Path)
    parser.add_argument("clusters", type=Path)
    parser.add_argument("--tables", type=Path)
    args = parser.parse_args()
    with args.results.open(encoding="utf8") as stream:
        rows = [json.loads(line) for line in stream]
    pages = json.loads(args.inventory.read_text())
    if collections.Counter(pages) != collections.Counter(row["page"] for row in rows):
        parser.error("results do not cover the exact manifest")
    from wpt_chrome import HOST
    subprocess.run(["node", HOST, "--verify"], check=True)
    counts, clusters = summarize(rows)
    if args.tables:
        args.tables.write_bytes(directory_tables(counts).encode())
    withheld = {r["page"].split("?")[0].split("#")[0] for r in rows
                if any(r[e]["status"] in {"CRASH", "TIMEOUT"} for e in ("hiwave", "chrome"))}
    args.expectations.write_bytes((json.dumps(counts, sort_keys=True, indent=2) + "\n").encode())
    details = [{"failure": reason, "likely_location": location, "subtests": item["count"],
                "files": sorted(item["files"] - withheld)}
               for (reason, location), item in sorted(clusters.items(), key=lambda x: (-x[1]["count"], x[0]))]
    args.clusters.write_bytes((json.dumps(details, indent=2) + "\n").encode())


if __name__ == "__main__":
    main()
