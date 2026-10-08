#!/usr/bin/env python3
"""Mutation sweep for n71 (text-metric provenance).

A guard that stays green without its fix is decoration. Each probe below
removes or inverts exactly one line of the change and asserts the named test
goes RED.
Every anchor is checked to occur EXACTLY once before any replacement: the
2026-09-30 sweep refused to start because a gap line was character-identical in
two sibling functions, and catching that before printing a result is now
machinery rather than a lesson (digest 2026-09-29, 2026-09-30).

    python3 trench/tools/n71_mutation_sweep.py
"""
import pathlib
import subprocess
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]

RS_TEXT = REPO / "crates/rustkit-layout/src/text.rs"
RS_ENGINE = REPO / "crates/rustkit-engine/src/lib.rs"
PY_GATE = REPO / "scripts/layout_oracle_gate.py"
PY_RECEIPT = REPO / "scripts/finish_line_receipt.py"

RUST = ("rust", "cargo test -p rustkit-engine --lib text_provenance_tests")
PYGATE = ("py", "python3 scripts/tests/test_layout_oracle_gate.py")
PYRECEIPT = ("py", "python3 scripts/tests/test_finish_line_receipt.py")

PROBES = [
    # (id, description, file, anchor, replacement, suite, tests expected RED)
    ("M1", "the stub build CLAIMS font-derived advances", RS_TEXT,
     'pub const TEXT_METRICS_ARE_FONT_DERIVED: bool = cfg!(any(windows, target_os = "macos"));',
     "pub const TEXT_METRICS_ARE_FONT_DERIVED: bool = true;",
     RUST, ["the_declared_backend_matches_what_shaping_actually_does"]),

    ("M2", "the stub build NAMES itself coretext", RS_TEXT,
     '''pub const TEXT_SHAPER_BACKEND: &str = if cfg!(windows) {
    "directwrite"
} else if cfg!(target_os = "macos") {
    "coretext"
} else {
    "stub-0.5em"
};''',
     'pub const TEXT_SHAPER_BACKEND: &str = "coretext";',
     RUST, ["the_declared_backend_matches_what_shaping_actually_does"]),

    ("M3", "the export drops text_backend", RS_ENGINE,
     '        "text_backend": rustkit_layout::TEXT_SHAPER_BACKEND,\n',
     "",
     RUST, ["the_layout_export_declares_its_text_shaper"]),

    ("M4", "the export drops text_metrics_font_derived", RS_ENGINE,
     '        "text_metrics_font_derived": rustkit_layout::TEXT_METRICS_ARE_FONT_DERIVED,\n',
     "",
     RUST, ["the_layout_export_declares_its_text_shaper"]),

    ("M5", "the export writes the boolean as a STRING", RS_ENGINE,
     '        "text_metrics_font_derived": rustkit_layout::TEXT_METRICS_ARE_FONT_DERIVED,',
     '        "text_metrics_font_derived": rustkit_layout::TEXT_METRICS_ARE_FONT_DERIVED.to_string(),',
     RUST, ["the_layout_export_declares_its_text_shaper"]),

    ("M6", "an absent provenance field reads as font-derived", PY_GATE,
     "    return str(backend), derived if isinstance(derived, bool) else None",
     "    return str(backend), True if derived is None else derived",
     PYGATE, ["test_a_capture_that_declares_no_provenance_is_not_trusted"]),

    ("M7", "a non-boolean provenance value reads as a yes", PY_GATE,
     "    return str(backend), derived if isinstance(derived, bool) else None",
     "    return str(backend), bool(derived) if derived is not None else None",
     PYGATE, ["test_a_capture_that_declares_no_provenance_is_not_trusted"]),

    ("M8", "gate_passes stops refusing an unattributable board", PY_GATE,
     '    if not report["summary"]["attributable"]:\n        return False\n',
     "",
     PYGATE, ["test_a_stub_shaper_capture_can_never_be_green_even_with_zero_failures"]),

    ("M9", "attributable treats 'did not say' as a yes", PY_GATE,
     '        "attributable": font_derived is True,',
     '        "attributable": font_derived is not False,',
     PYGATE, ["test_a_capture_that_declares_no_provenance_is_not_trusted"]),

    ("M10", "the exposure classifier ALSO claims mere ancestry", PY_GATE,
     '            if key not in exposure:',
     '            if key not in exposure and False:',
     PYGATE, ["test_text_exposure_claims_downward_and_sideways_but_never_ancestry"]),

    ("M10b", "the exposure classifier claims EVERY box (ancestry included)", PY_GATE,
     "    visit(root, ())\n\n    def visit_flow",
     "    visit(root, ())\n    exposure.update({k: \"own\" for k in subtree_has_text})\n\n    def visit_flow",
     PYGATE, ["test_text_exposure_claims_downward_and_sideways_but_never_ancestry"]),

    ("M11", "the exposure classifier drops the FLOW relation", PY_GATE,
     "    visit_flow(root, ())\n    return exposure",
     "    return exposure",
     PYGATE, ["test_text_exposure_claims_downward_and_sideways_but_never_ancestry"]),

    ("M11b", "FLOW looks at a preceding text BOX, not a preceding subtree", PY_GATE,
     "                    if subtree_has_text.get(sib):",
     "                    if (children[earlier].get('type') == 'text'):",
     PYGATE, ["test_text_exposure_claims_downward_and_sideways_but_never_ancestry"]),

    ("M12", "OWN narrows to a direct text CHILD only", PY_GATE,
     "        sub = node.get(\"type\") == \"text\"\n        for index, child in enumerate(node.get(\"children\") or []):\n            sub = visit(child, path + (index,)) or sub\n        subtree_has_text[key] = sub\n        if sub:\n            exposure[key] = \"own\"",
     "        sub = node.get(\"type\") == \"text\"\n        direct = sub\n        for index, child in enumerate(node.get(\"children\") or []):\n            sub = visit(child, path + (index,)) or sub\n            direct = direct or child.get(\"type\") == \"text\"\n        subtree_has_text[key] = sub\n        if direct:\n            exposure[key] = \"own\"",
     PYGATE, ["test_text_exposure_claims_downward_and_sideways_but_never_ancestry"]),

    ("M12b", "OWN dropped entirely", PY_GATE,
     '        if sub:\n            exposure[key] = "own"\n',
     "",
     PYGATE, ["test_text_exposure_claims_downward_and_sideways_but_never_ancestry",
              "test_the_exposure_count_never_silently_corrects_the_failure_count"]),

    ("M14", "the receipt stops refusing an unattributable geometry column", PY_RECEIPT,
     '    if case.get("attributable") is not True:\n        return _unmeasured(\n            "text_metrics_not_font_derived",\n            text_backend=case.get("text_backend", "unknown"),\n        )\n',
     "",
     PYRECEIPT, ["test_a_stub_shaper_board_produces_no_n_over_26_at_all"]),

    ("M15", "the receipt scores an unattributable column RED instead of UNMEASURED", PY_RECEIPT,
     '    if case.get("attributable") is not True:\n        return _unmeasured(\n            "text_metrics_not_font_derived",\n            text_backend=case.get("text_backend", "unknown"),\n        )',
     '    if case.get("attributable") is not True:\n        return {"measured": True, "green": False, "reason": None}',
     PYRECEIPT, ["test_a_stub_shaper_board_produces_no_n_over_26_at_all"]),

    ("M16", "the receipt reads 'did not say' as font-derived", PY_RECEIPT,
     '    if case.get("attributable") is not True:',
     '    if case.get("attributable") is False:',
     PYRECEIPT, ["test_a_stub_shaper_board_produces_no_n_over_26_at_all"]),

    ("M13", "the headline failure count is NETTED of exposure", PY_GATE,
     '        "geometry_failures": len(geometry_failures),\n        "join_failures": len(join_failures),\n        "text_backend": backend,',
     '        "geometry_failures": len(geometry_failures) - text_exposed,\n        "join_failures": len(join_failures),\n        "text_backend": backend,',
     PYGATE, ["test_the_exposure_count_never_silently_corrects_the_failure_count"]),
]


def run(cmd):
    return subprocess.run(cmd, shell=True, cwd=REPO, capture_output=True, text=True)


def red_tests(suite):
    """Names of tests that FAILED under `suite`, or None when all passed."""
    kind, cmd = suite
    proc = run(cmd)
    if proc.returncode == 0:
        return set()
    out = proc.stdout + proc.stderr
    failed = set()
    if kind == "rust":
        for line in out.splitlines():
            line = line.strip()
            if line.startswith("test ") and line.endswith("FAILED"):
                failed.add(line.split()[1].split("::")[-1])
            if line.startswith("---- ") and " stdout" in line:
                failed.add(line.split()[1].split("::")[-1])
    else:
        # The python suites run tests in sorted order and die on the first
        # assertion, naming it in the traceback.
        for line in out.splitlines():
            if ", in test_" in line:
                failed.add(line.rsplit(", in ", 1)[1].strip())
    return failed or {"<suite failed, test name not parsed>"}


def main():
    print("n71 mutation sweep — text-metric provenance")

    # Control.
    for suite in (RUST, PYGATE, PYRECEIPT):
        if red_tests(suite):
            print(f"CONTROL RED before any mutation ({suite[0]}) — aborting")
            return 1
    print("control: both suites GREEN\n")

    # Every anchor must be unique BEFORE anything is touched.
    for pid, desc, path, anchor, _repl, _suite, _want in PROBES:
        n = path.read_text().count(anchor)
        if n != 1:
            print(f"{pid}: anchor occurs {n} times in {path.name}, not once — aborting")
            return 1
    print(f"all {len(PROBES)} anchors occur exactly once\n")

    results = []
    killed_by = {}
    for pid, desc, path, anchor, repl, suite, want in PROBES:
        original = path.read_text()
        path.write_text(original.replace(anchor, repl, 1))
        try:
            failed = red_tests(suite)
        finally:
            path.write_text(original)
        verdict = "RED " if failed else "GREEN"
        caught = sorted(failed & set(want))
        unexpected = sorted(failed - set(want))
        results.append((pid, verdict, desc, caught, unexpected))
        for t in failed:
            killed_by.setdefault(t, []).append(pid)
        extra = f"  (also: {', '.join(unexpected)})" if unexpected else ""
        print(f"{pid:4} {verdict}  {desc}")
        if failed:
            print(f"       caught by: {', '.join(caught) or '(NOT the named guard)'}{extra}")

    # Control again: the tree must be restored.
    print()
    for suite in (RUST, PYGATE, PYRECEIPT):
        if red_tests(suite):
            print(f"CONTROL RED after the sweep ({suite[0]}) — the tree was not restored")
            return 1
    print("control: both suites GREEN again (tree restored)\n")

    survivors = [r for r in results if r[1] == "GREEN"]
    misattributed = [r for r in results if r[1] == "RED " and not r[3]]
    print(f"{len(results) - len(survivors)}/{len(results)} probes RED")
    if survivors:
        print("SURVIVORS (the change has lines no guard asserts on):")
        for pid, _v, desc, _c, _u in survivors:
            print(f"  {pid}: {desc}")
    if misattributed:
        print("RED but NOT by the named guard (the guard may be decoration):")
        for pid, _v, desc, _c, unexpected in misattributed:
            print(f"  {pid}: {desc} -> {', '.join(unexpected)}")

    print("\nkilled_by, per guard (a guard no probe kills is decoration):")
    all_guards = sorted({t for _p, _d, _f, _a, _r, _s, want in PROBES for t in want})
    for guard in all_guards:
        probes = killed_by.get(guard, [])
        print(f"  {guard:66} {', '.join(probes) or 'NOTHING'}")

    return 1 if survivors or misattributed else 0


if __name__ == "__main__":
    sys.exit(main())
