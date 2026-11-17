"""Aggregate a complete paired run. Cluster only matched, named assertions."""
import argparse
import collections
import json
import re
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
    if page.endswith("/test_font_family_parsing.html"):
        if "shouldn't parse" in message:
            return "Invalid font-family tokens retained in stylesheet", CSS + " font-family parser / CSSOM rule readback"
        if "should parse" in message or "serialization" in message:
            return "Valid font-family parsed or serialized differently", CSS + " font-family parser / CSSOM rule readback"
    if "@font-feature-values" in name and "bad rule type expected 14 but got 0" in message:
        return "Font feature values rule has incorrect CSSRule type", CSSOM + " rule type dispatch"
    if page.split("?")[0].endswith("/font-shorthand-serialization-prevention.html") and 'expected "" but got' in message:
        return "Font shorthand serialized despite incompatible longhands", CSSOM + " shorthand readback"
    if message.startswith("assert_equals:") and "data-" in message:
        geometry = re.search(r"\n(offset(?:Top|Left|Width|Height)|client(?:Height|Width)|scroll(?:Height|Width)|width|height) expected", message)
        if geometry:
            section = page.strip("/").split("/")[1]
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
            reference[test["name"]].append(test["status"])
        for test in a["subtests"]:
            matches = reference[test["name"]]
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("inventory", type=Path)
    parser.add_argument("results", type=Path)
    parser.add_argument("expectations", type=Path)
    parser.add_argument("clusters", type=Path)
    args = parser.parse_args()
    with args.results.open(encoding="utf8") as stream:
        rows = [json.loads(line) for line in stream]
    pages = json.loads(args.inventory.read_text())
    if collections.Counter(pages) != collections.Counter(row["page"] for row in rows):
        parser.error("results do not cover the exact manifest")
    counts, clusters = summarize(rows)
    withheld = {r["page"].split("?")[0].split("#")[0] for r in rows
                if any(r[e]["status"] in {"CRASH", "TIMEOUT"} for e in ("hiwave", "chrome"))}
    args.expectations.write_bytes((json.dumps(counts, sort_keys=True, indent=2) + "\n").encode())
    details = [{"failure": reason, "likely_location": location, "subtests": item["count"],
                "files": sorted(item["files"] - withheld)}
               for (reason, location), item in sorted(clusters.items(), key=lambda x: (-x[1]["count"], x[0]))]
    args.clusters.write_bytes((json.dumps(details, indent=2) + "\n").encode())


if __name__ == "__main__":
    main()
