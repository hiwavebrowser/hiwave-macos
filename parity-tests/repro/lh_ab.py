#!/usr/bin/env python3
"""Session-7 A/B: h1 line-box height, Chrome (oracle path, reset-injected) vs rustkit.

For each fixture under parity-tests/repro/websuite/micro/<case>/index.html:
  1. Chrome: captureChrome PNG + exportStyles rects (same deterministic context
     the chrome-148 baselines were captured with).
  2. rustkit: parity-capture --dump-frame/--dump-layout (same invocation
     parity_test.py uses; reset injection fires on the /websuite/micro/ path).
  3. Report the h1 rect height from Chrome rects, the h1 band height measured
     from pixels in BOTH images (rows containing the #dd3333 band), and
     rustkit's h1 box height from layout.json.

Run from the hiwave-macos repo root.
"""
import json
import os
import struct
import subprocess
import sys
from pathlib import Path

REPO = Path.cwd()
REPRO = REPO / "parity-tests/repro"
OUT = REPRO / "lh-ab-out"
OUT.mkdir(exist_ok=True)

CASES = ["h1band", "h1normal"]
W, H = 800, 400


def node(script: str) -> str:
    env = {**os.environ, "PATH": f"/opt/homebrew/bin:{os.environ.get('PATH', '')}"}
    r = subprocess.run(["node", "--input-type=module", "-e", script],
                       capture_output=True, text=True, timeout=120, cwd=REPO, env=env)
    if r.returncode != 0:
        raise RuntimeError(f"node failed: {r.stderr[-1500:]}")
    return r.stdout


def chrome_capture(html: Path, png: Path, rects: Path):
    node(f"""
import {{ captureChrome }} from './tools/parity_oracle/capture_chrome.mjs';
import {{ exportStyles }} from './tools/parity_oracle/export_styles.mjs';
import {{ writeFileSync }} from 'fs';
await captureChrome('{html}', '{png}', {W}, {H});
const styles = await exportStyles('{html}', {W}, {H}, 'h1, .marker, body, html');
writeFileSync('{rects}', JSON.stringify(styles, null, 2));
console.log('ok');
""")


def rustkit_capture(html: Path, ppm: Path, layout: Path):
    r = subprocess.run([
        "./target/release/parity-capture",
        "--html-file", str(html),
        "--width", str(W), "--height", str(H),
        "--dump-frame", str(ppm),
        "--dump-layout", str(layout),
    ], capture_output=True, text=True, timeout=120, cwd=REPO)
    if r.returncode != 0:
        raise RuntimeError(f"parity-capture failed: {r.stderr[-1500:]}")


def read_png_rows(path: Path):
    """Return list of row pixel-lists via node (avoids PIL dependency drift)."""
    out = node(f"""
import {{ PNG }} from './tools/parity_oracle/node_modules/pngjs/lib/png.js';
import {{ readFileSync }} from 'fs';
const png = PNG.sync.read(readFileSync('{path}'));
const rows = [];
for (let y = 0; y < png.height; y++) {{
  let red = 0, blue = 0;
  for (let x = 0; x < png.width; x++) {{
    const i = (y * png.width + x) * 4;
    const r = png.data[i], g = png.data[i+1], b = png.data[i+2];
    if (r > 150 && g < 110 && b < 110) red++;
    if (b > 150 && g < 110 && r < 110) blue++;
  }}
  rows.push([red, blue]);
}}
console.log(JSON.stringify(rows));
""")
    return json.loads(out.strip().splitlines()[-1])


def read_ppm_rows(path: Path):
    data = path.read_bytes()
    # P6 header: magic, dims, maxval
    parts = data.split(b"\n", 3)
    if parts[0].strip() != b"P6":
        raise ValueError("not P6")
    w, h = map(int, parts[1].split())
    body = parts[3] if len(parts) > 3 else b""
    rows = []
    for y in range(h):
        red = blue = 0
        base = y * w * 3
        row = body[base:base + w * 3]
        for x in range(w):
            r, g, b = row[x * 3], row[x * 3 + 1], row[x * 3 + 2]
            if r > 150 and g < 110 and b < 110:
                red += 1
            if b > 150 and g < 110 and r < 110:
                blue += 1
        rows.append([red, blue])
    return rows


def band(rows, idx, min_px=50):
    ys = [y for y, c in enumerate(rows) if c[idx] >= min_px]
    if not ys:
        return None
    return (min(ys), max(ys), max(ys) - min(ys) + 1)


def main():
    report = {}
    for case in CASES:
        html = REPRO / "websuite/micro" / case / "index.html"
        png = OUT / f"{case}-chrome.png"
        rects = OUT / f"{case}-chrome-rects.json"
        ppm = OUT / f"{case}-rustkit.ppm"
        layout = OUT / f"{case}-rustkit.layout.json"

        chrome_capture(html, png, rects)
        rustkit_capture(html, ppm, layout)

        crows = read_png_rows(png)
        rrows = read_ppm_rows(ppm)
        entry = {
            "chrome_red_band": band(crows, 0),
            "chrome_blue_band": band(crows, 1),
            "rustkit_red_band": band(rrows, 0),
            "rustkit_blue_band": band(rrows, 1),
        }
        # Chrome computed rects for h1
        rdata = json.loads(rects.read_text())
        if isinstance(rdata, dict):
            rdata = rdata.get("elements", rdata.get("styles", []))
        for el in rdata:
            sel = el.get("selector", "")
            if sel.startswith("h1"):
                entry["chrome_h1_rect"] = el.get("rect")
                st = el.get("styles", {})
                entry["chrome_h1_lineHeight"] = st.get("line-height", st.get("lineHeight"))
                entry["chrome_h1_fontSize"] = st.get("font-size", st.get("fontSize"))
                entry["chrome_h1_fontFamily"] = st.get("font-family", st.get("fontFamily"))
        report[case] = entry
        print(json.dumps({case: entry}, indent=2))

    (OUT / "report.json").write_text(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
