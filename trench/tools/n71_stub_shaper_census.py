#!/usr/bin/env python3
"""How much of this seat's Gate A board is produced by the STUB text shaper?

NOT A RECEIPT, and this one invalidates rather than measures.

`TextShaper::shape` in crates/rustkit-layout/src/text.rs has three bodies. The
`#[cfg(windows)]` one calls DirectWrite and the `#[cfg(target_os = "macos")]`
one calls Core Text. The third --- `#[cfg(all(not(windows), not(target_os =
"macos")))]`, line ~1635, the one this Linux trench seat compiles --- is a stub:

    let avg_char_width = size * 0.5;
    let advance = if c.is_ascii() { avg_char_width } else { size };

It reads no font, and it returns `Ok`, so every caller believes it shaped. On
this seat RustKit therefore measures all text with a 0.5em-per-ASCII-character
ruler.

This matters beyond "text is wrong". The seat control
(`tools/parity_oracle/capture_seat_control.mjs`) exists to subtract the platform
confound by putting the SEAT's fonts on both sides. It cannot: RustKit's side
has no fonts in it at all. So `Delta_real = RustKit_seat - Chrome_seat` is not a
RustKit box-math defect wherever text feeds the box --- it is the stub, and the
seat control's whole argument does not reach it.

This script measures the exposure. For every text box in a capture it compares
the dumped width against the stub's closed form; agreement to 1e-3 means the
number came from the stub and not from any font.

    python3 trench/tools/n71_stub_shaper_census.py <captures dir> [gate-a.json]
"""
import json
import pathlib
import sys


def walk(node, depth=0):
    yield depth, node
    for c in node.get("children") or []:
        yield from walk(c, depth + 1)


def stub_width(text, size):
    """The Linux stub's closed form, transcribed from text.rs:1645."""
    return sum(size * 0.5 if c.isascii() else size for c in text)


def main():
    root = pathlib.Path(sys.argv[1])
    print("n71 stub-shaper census — NOT A RECEIPT; this one INVALIDATES")
    print(f"{'case':24}{'text':>7}{'stub':>7}{'other':>7}  note")
    tot_text = tot_stub = 0
    for case_dir in sorted(root.iterdir()):
        lj = case_dir / "layout.json"
        if not lj.exists():
            continue
        doc = json.load(open(lj))
        n_text = n_stub = 0
        for _, node in walk(doc["root"]):
            if node.get("type") != "text":
                continue
            text = node.get("text")
            rect = node.get("rect")
            if text is None or rect is None:
                continue
            n_text += 1
            # The dump carries no font-size, so solve for the size the stub
            # would have needed and accept only a plausible CSS pixel size.
            # A single-line run's width is monotone in size, so this is exact.
            ascii_n = sum(1 for c in text if c.isascii())
            wide_n = len(text) - ascii_n
            denom = ascii_n * 0.5 + wide_n
            if denom <= 0:
                continue
            implied = rect["width"] / denom
            if abs(implied - round(implied, 3)) < 1e-9 and 1.0 <= implied <= 200.0:
                # width == stub_width(text, implied) by construction; the test
                # that matters is whether `implied` is a believable font size.
                if abs(stub_width(text, implied) - rect["width"]) < 1e-3:
                    n_stub += 1
        tot_text += n_text
        tot_stub += n_stub
        note = "" if n_stub == n_text else "<- unmatched; see WRAPPED note below"
        print(f"{case_dir.name:24}{n_text:>7}{n_stub:>7}{n_text - n_stub:>7}  {note}")
    print(f"{'TOTAL':24}{tot_text:>7}{tot_stub:>7}{tot_text - tot_stub:>7}")
    pct = 100.0 * tot_stub / tot_text if tot_text else 0.0
    print(f"\n{tot_stub} of {tot_text} text runs ({pct:.2f}%) have a width the "
          f"stub's closed form reproduces exactly.")
    print("The unmatched remainder is NOT evidence of real shaping. A run that")
    print("WRAPS is sized by its container, not by the sum of its advances, so")
    print("the closed form cannot reproduce it either way. The direct evidence")
    print("that the stub is what runs is the source at text.rs:1635 and the")
    print("probe: shape(\" \") returns advance 8.0 and shape(\"mm\") returns")
    print("16.0 for EVERY family asked, including families not installed.")

    if len(sys.argv) > 2:
        doc = json.load(open(sys.argv[2]))
        # Exposure, stated narrowly so it cannot be read as more than it is.
        # "Every box has text somewhere beneath body" is true and useless, so
        # this counts only the two direct relationships:
        #   OWN   the failing box IS a text box, or has a text CHILD -- its
        #         content size is a text measurement
        #   FLOW  a PRECEDING sibling subtree contains text -- inline flow
        #         hands that advance along to this box's inline position
        total = own = flow = 0
        for case in doc["cases"]:
            cid = case["case_id"]
            cap = root / cid / "layout.json"
            if not cap.exists():
                continue
            d = json.load(open(cap))
            is_text, subtree_text, kids = {}, {}, {}

            def mark(node, path=()):
                key = ".".join(map(str, path))
                is_text[key] = node.get("type") == "text"
                children = node.get("children") or []
                kids[key] = len(children)
                sub = is_text[key]
                for i, c in enumerate(children):
                    sub |= mark(c, path + (i,))
                subtree_text[key] = sub
                return sub

            mark(d["root"])
            for f in case.get("failures", []):
                if f.get("axis") not in ("x", "y", "width", "height"):
                    continue
                total += 1
                p_ = f["path"]
                parts = p_.split(".")
                direct = is_text.get(p_, False) or any(
                    is_text.get(f"{p_}.{i}", False) for i in range(kids.get(p_, 0))
                )
                if direct:
                    own += 1
                    continue
                if len(parts) > 1:
                    parent = ".".join(parts[:-1])
                    mine = int(parts[-1])
                    if any(subtree_text.get(f"{parent}.{i}", False) for i in range(mine)):
                        flow += 1

        def pct(n):
            return 100.0 * n / total if total else 0.0

        print(f"\nGate A exposure over {total} geometry failures:")
        print(f"  OWN  (is a text box, or has a text child): {own:>5}  {pct(own):5.2f}%")
        print(f"  FLOW (a preceding sibling carries text):   {flow:>5}  {pct(flow):5.2f}%")
        print(f"  neither of those two direct relations:     {total - own - flow:>5}  "
              f"{pct(total - own - flow):5.2f}%")
        print("  (the third row is NOT clean: intrinsic sizing propagates a text")
        print("   measurement upward through any ancestor, which this does not count)")


if __name__ == "__main__":
    main()
