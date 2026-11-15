import subprocess, re, sys, html, os, json
CH = r"C:\Program Files\Google\Chrome\Application\chrome.exe"
url = "file:///" + os.path.abspath("matrix.html").replace("\\", "/")
res = {}
for dpr in (1, 2):
    for w in (500, 800):
        udd = os.path.abspath(f"udd_{dpr}_{w}")
        out = subprocess.run([CH, "--headless=new", "--disable-gpu", "--no-sandbox", f"--user-data-dir={udd}",
                              f"--force-device-scale-factor={dpr}", f"--window-size={w},900",
                              "--virtual-time-budget=5000", "--dump-dom", url],
                             capture_output=True, text=True, timeout=120).stdout
        m = re.search(r'<pre id="out">(.*?)</pre>', out, re.S)
        rows = html.unescape(m.group(1)).strip().split("\n") if m else ["NO OUTPUT: " + out[:200]]
        res[f"dpr{dpr}_w{w}"] = rows
json.dump(res, open("chrome157.json", "w"), indent=1)
keys = list(res)
print("id | " + " | ".join(keys))
ids = [r.split("|")[0] for r in res[keys[0]]]
for i, id_ in enumerate(ids):
    print(id_, "|", " | ".join("|".join(res[k][i].split("|")[1:]) if i < len(res[k]) else "?" for k in keys))
