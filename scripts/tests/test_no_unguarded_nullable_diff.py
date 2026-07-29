"""Static guard: no unguarded use of a nullable diff field.

Why this file exists, plainly: the NOT-MEASURED state was introduced in one
change set and then broken FOUR times in that same change set — a display
format string, two comparison tools, `compare_reports` arithmetic, and finally
the scout print loops. Three of those were caught by a reviewer, one by CI,
none by the author. Each round was fixed by hand and each round missed more.

Trying harder was not working, so this replaces effort with enumeration. It
walks the AST of every parity script and fails on any format, arithmetic, or
ordering comparison applied to a field that can be None, unless the site is
listed below as reviewed-and-guarded.

If you add a new consumer and this fails: do not add it to the allowlist to
make the test pass. Guard the site, then add it with a reason.

Run: python3 scripts/tests/test_no_unguarded_nullable_diff.py
"""
import ast
import os
import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
SCRIPTS = REPO / "scripts"

# Fields that carry None when the instrument refused to measure.
NULLABLE = {
    "diff_pct", "diff_pct_median", "diff_pct_min", "diff_pct_max",
    "avg_diff", "avg_diff_pct",
}

# Sites reviewed and confirmed guarded. `file: {line-independent reason}` —
# keyed by (file, enclosing function) rather than line number so ordinary
# edits do not silently invalidate the review.
ALLOWED = {
    ("parity_swarm.py", "fmt_diff"): "the None check IS this function",
    ("parity_test.py", "_fmt"): "the None check IS this function",
    ("parity_aggregate.py", "compare_reports"): "early `continue` on either side None",
    ("parity_lib.py", "aggregate_iterations"): "all uses sit inside `if diffs:`",
    ("parity_lib.py", "execute_work_unit"): "guarded by `is not None` before compare",
    ("parity_swarm.py", "run_exploit_phase"): "guarded by `is not None` in the filter",
    ("parity_test.py", "run_test"): "guarded by `is not None` before compare",
}


class Scan(ast.NodeVisitor):
    def __init__(self):
        self.hits = []
        self._fn = []

    def visit_FunctionDef(self, node):
        self._fn.append(node.name)
        self.generic_visit(node)
        self._fn.pop()

    visit_AsyncFunctionDef = visit_FunctionDef

    def _field(self, node):
        if isinstance(node, ast.Attribute) and node.attr in NULLABLE:
            return node.attr
        if (isinstance(node, ast.Subscript) and isinstance(node.slice, ast.Constant)
                and node.slice.value in NULLABLE):
            return node.slice.value
        if isinstance(node, ast.Name) and node.id in NULLABLE:
            return node.id
        return None

    def _record(self, node, what):
        self.hits.append((node.lineno, self._fn[-1] if self._fn else "<module>", what))

    def visit_FormattedValue(self, node):
        field = self._field(node.value)
        if field and node.format_spec is not None:
            self._record(node, f"format spec applied to `{field}`")
        self.generic_visit(node)

    def visit_BinOp(self, node):
        if isinstance(node.op, (ast.Sub, ast.Add, ast.Mult, ast.Div)):
            for side in (node.left, node.right):
                field = self._field(side)
                if field:
                    self._record(node, f"arithmetic on `{field}`")
        self.generic_visit(node)

    def visit_Compare(self, node):
        field = self._field(node.left)
        if field and any(isinstance(o, (ast.Lt, ast.Gt, ast.LtE, ast.GtE)) for o in node.ops):
            self._record(node, f"ordering comparison on `{field}`")
        self.generic_visit(node)


def test_no_unguarded_nullable_diff_use():
    offenders = []
    for path in sorted(SCRIPTS.rglob("*.py")):
        if "tests" in path.parts or "__pycache__" in path.parts:
            continue
        scan = Scan()
        scan.visit(ast.parse(path.read_text(errors="replace")))
        for lineno, fn, what in scan.hits:
            if (path.name, fn) in ALLOWED:
                continue
            offenders.append(f"{path.name}:{lineno} in {fn}(): {what}")

    assert not offenders, (
        "unguarded use of a field that can be None:\n  "
        + "\n  ".join(offenders)
        + "\n\nGuard the site, then add (file, function) to ALLOWED with a "
          "reason. Do NOT allowlist it to make this pass."
    )


def test_allowlist_entries_still_exist():
    """A stale allowlist is worse than none — it grants an exemption to a
    function that no longer exists while reading as reviewed."""
    missing = []
    for (filename, fn) in ALLOWED:
        path = SCRIPTS / filename
        if not path.exists():
            missing.append(f"{filename} (file gone)")
            continue
        names = {
            n.name for n in ast.walk(ast.parse(path.read_text(errors="replace")))
            if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef))
        }
        if fn not in names:
            missing.append(f"{filename}:{fn}() (function gone)")
    assert not missing, "stale ALLOWED entries: " + ", ".join(missing)


if __name__ == "__main__":
    os.chdir(REPO)
    for name, fn in sorted(globals().items()):
        if name.startswith("test_") and callable(fn):
            fn()
            print(f"ok  {name}")
    print("PASS: every nullable-diff consumer is guarded or reviewed")
