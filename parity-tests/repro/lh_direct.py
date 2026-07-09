import subprocess, json, os, tempfile, pathlib
REPO = pathlib.Path("/Users/petecopeland/Repos/hiwave/hiwave-macos")
CAP = REPO / "target" / "release" / "parity-capture"
CASES = {
  "h1-num": '<h1 style="margin:0;font-size:2em;line-height:1.5">Heading</h1>',
  "h1-px":  '<h1 style="margin:0;font-size:2em;line-height:60px">Heading</h1>',
  "h1-pct": '<h1 style="margin:0;font-size:2em;line-height:150%">Heading</h1>',
  "h1-em":  '<h1 style="margin:0;font-size:2em;line-height:1.5em">Heading</h1>',
}
def find(node, out):
    if isinstance(node, dict):
        bb = node.get("border_box")
        if bb and node.get("children"):
            for c in node["children"]:
                if isinstance(c, dict) and c.get("type") == "text":
                    out.append((c.get("text", "")[:10], bb.get("height")))
        for c in node.get("children", []) or []:
            find(c, out)
for name, frag in CASES.items():
    html = '<!DOCTYPE html><html style="font-size:16px"><head><meta charset=utf-8></head><body style="margin:0">' + frag + '</body></html>'
    with tempfile.NamedTemporaryFile("w", suffix=".html", delete=False, dir=str(REPO)) as f:
        f.write(html); p = f.name
    o = REPO / "parity-tests" / "repro" / "lh-out"; o.mkdir(parents=True, exist_ok=True)
    lay = o / (name + ".json")
    r = subprocess.run([str(CAP), "--html-file", p, "--width", "800", "--height", "600",
                        "--dump-frame", str(o / (name + ".ppm")), "--dump-layout", str(lay)],
                       capture_output=True, text=True, timeout=60, cwd=str(REPO))
    os.unlink(p)
    if r.returncode != 0:
        print(name, "FAIL", r.stderr[:150]); continue
    out = []; find(json.load(open(lay)).get("root"), out)
    print(name, out)
