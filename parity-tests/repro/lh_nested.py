import subprocess, json, os, tempfile, pathlib
REPO = pathlib.Path("/Users/petecopeland/Repos/hiwave/hiwave-macos")
CAP = REPO / "target" / "release" / "parity-capture"
CASES = {
  # div -> div inheritance (both default font 16px). inner text line box should be 24 if inherited.
  "div-div": '<div style="line-height:1.5"><div id=t>Xy</div></div>',
  # body -> div
  "body-div": '<div id=t>Xy</div>',  # body has line-height set below
  # grandparent set, skip a level
  "gp-skip": '<div style="line-height:1.5"><section><div id=t>Xy</div></section></div>',
}
def find(node, out):
    if isinstance(node, dict):
        bb = node.get("border_box")
        if bb and node.get("children"):
            for c in node["children"]:
                if isinstance(c, dict) and c.get("type") == "text":
                    out.append((c.get("text", "")[:6], round(bb.get("height",0),2)))
        for c in node.get("children", []) or []:
            find(c, out)
def run(name, frag, bodystyle="margin:0"):
    html = '<!DOCTYPE html><html style="font-size:16px"><head><meta charset=utf-8></head><body style="'+bodystyle+'">' + frag + '</body></html>'
    with tempfile.NamedTemporaryFile("w", suffix=".html", delete=False, dir=str(REPO)) as f:
        f.write(html); p = f.name
    o = REPO / "parity-tests" / "repro" / "lh-out"; o.mkdir(parents=True, exist_ok=True)
    lay = o / (name + ".json")
    r = subprocess.run([str(CAP), "--html-file", p, "--width", "800", "--height", "600",
                        "--dump-frame", str(o/(name+".ppm")), "--dump-layout", str(lay)],
                       capture_output=True, text=True, timeout=60, cwd=str(REPO))
    os.unlink(p)
    if r.returncode != 0: print(name, "FAIL", r.stderr[:120]); return
    out = []; find(json.load(open(lay)).get("root"), out)
    print(name, "->", out, "(expect inherited=24, normal=19.2)")
run("div-div", CASES["div-div"])
run("body-div", CASES["body-div"], bodystyle="margin:0;line-height:1.5")
run("gp-skip", CASES["gp-skip"])
