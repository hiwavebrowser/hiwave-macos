"""Enumerate CSS test URLs using the source classifier in the pinned WPT checkout."""
import argparse
import json
import sys
from pathlib import Path

DIRECTORIES = (
    "css-values css-cascade cssom css-color css-variables selectors css-box css-display "
    "css-flexbox css-grid css-transforms css-text css-fonts css-backgrounds css-sizing "
    "css-position css-overflow"
).split()


def inventory(root):
    sys.path.insert(0, str(root / "tools"))
    import localpaths  # WPT's vendored manifest dependencies
    from manifest.sourcefile import SourceFile
    pages = []
    for directory in DIRECTORIES:
        for path in sorted((root / "css" / directory).rglob("*")):
            if not path.is_file():
                continue
            source = SourceFile(str(root), str(path.relative_to(root)), "/")
            kind, entries = source.manifest_items()
            if kind == "testharness":
                pages.extend(entry.url for entry in entries)
    return sorted(pages)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    args = parser.parse_args()
    print(json.dumps(inventory(args.root.resolve()), indent=2))
